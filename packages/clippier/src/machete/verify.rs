//! Deliberately opt-in finite compiler checks in disposable complete project copies.
//!
//! Copies are not a security sandbox: opting in executes build scripts and proc
//! macros with the caller's OS privileges. This lane refuses symlinks, project
//! Cargo configuration, external local paths and automatic toolchain selection.
//! It uses an empty Cargo home (offline cache misses are environment failures).
use super::{
    BoxError, MacheteOptions, MachetePlan,
    apply::{validate_evidence, validate_plan},
    discover::plan_machete_candidates,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Declared Cargo operations; no shell or caller-supplied command-line fragments.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationOperation {
    /// Type check without linking.
    #[default]
    Check,
    /// Compile selected targets without running tests.
    TestNoRun,
    /// Compile selected targets.
    Build,
}

/// One finite compiler configuration. It is not universal dependency absence proof.
#[allow(clippy::struct_excessive_bools)] // Independent Cargo command switches, not mutually exclusive states.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VerificationProfile {
    /// Unique human-readable coverage label.
    pub name: String,
    /// Declared Cargo operation.
    pub operation: VerificationOperation,
    /// Exact Cargo package names; empty selects workspace defaults.
    pub packages: Vec<String>,
    /// Exact feature names.
    pub features: Vec<String>,
    /// Disable default features.
    pub no_default_features: bool,
    /// Enable all features (mutually exclusive with explicit features).
    pub all_features: bool,
    /// Include all Cargo target kinds.
    pub all_targets: bool,
    /// Explicit installed target triple, if any; never installs one.
    pub target: Option<String>,
    /// Use Cargo's release profile.
    pub release: bool,
}

/// Execution policy. Default is non-executing and must be deliberately changed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VerificationOptions {
    /// Acknowledge execution of untrusted project build scripts/proc macros.
    pub allow_execution: bool,
    /// Absolute real Cargo binary, not a rustup proxy or shell wrapper.
    pub cargo_path: PathBuf,
    /// Absolute real rustc binary from the same selected installed toolchain.
    pub rustc_path: PathBuf,
    /// Optional explicitly selected native linker for build scripts and proc macros.
    /// Passed as a single encoded rustc option; ambient compiler flags are never inherited.
    pub linker_path: Option<PathBuf>,
    /// Selected finite configurations; at least one is required.
    pub profiles: Vec<VerificationProfile>,
    /// Check the complete removal set.
    pub combined: bool,
    /// Also check each manifest change in isolation (not each dependency in a manifest).
    pub individual: bool,
    /// Per-command deadline in milliseconds.
    pub timeout_ms: u64,
    /// Retained bytes per output stream; excess bytes are drained and discarded.
    pub max_output_bytes: usize,
}
impl Default for VerificationOptions {
    fn default() -> Self {
        Self {
            allow_execution: false,
            cargo_path: PathBuf::new(),
            rustc_path: PathBuf::new(),
            linker_path: None,
            profiles: Vec::new(),
            combined: true,
            individual: false,
            timeout_ms: 120_000,
            max_output_bytes: 64 * 1024,
        }
    }
}

/// Outcome of a command, separate from the inference made from a baseline.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationStatus {
    /// Cargo exited successfully.
    Passed,
    /// Cargo returned a nonzero exit code.
    Failed,
    /// A deadline expired; the process group was killed.
    TimedOut,
    /// Spawning, waiting or capturing failed.
    EnvironmentFailure,
    /// Candidate deliberately not executed because its baseline failed.
    NotRun,
}

/// Bounded execution record.
#[derive(Debug, Clone, Serialize)]
pub struct VerificationCommandReport {
    /// Exact argument vector (without a shell).
    pub arguments: Vec<String>,
    /// Command outcome.
    pub status: VerificationStatus,
    /// Exit status if available.
    pub exit_code: Option<i32>,
    /// Bounded stdout; invalid UTF-8 is replaced.
    pub stdout: String,
    /// Bounded stderr or environment error.
    pub stderr: String,
    /// True when either captured stream exceeded its bound.
    pub output_truncated: bool,
}

/// Finite scoped inference, never source usage or permission to apply.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationOutcome {
    /// Baseline passed and the selected manifest replacements compiled.
    CandidateCompiled,
    /// Baseline passed and the selected candidate compiler command failed.
    /// This is not proof that a dependency is used.
    CandidateFailed,
    /// Baseline/preparation/environment prevented a compiler comparison.
    Unknown,
}

/// Baseline and selected candidate for one profile.
#[derive(Debug, Clone, Serialize)]
pub struct VerificationProfileReport {
    /// Exact finite configuration.
    pub profile: VerificationProfile,
    /// Candidate scope: combined or individual manifest path.
    pub candidate: String,
    /// Exact absolute manifests replaced by this scope; individual means manifest,
    /// not one dependency declaration.
    pub manifests: Vec<PathBuf>,
    /// Scoped compiler comparison; never global removal authorization.
    pub outcome: VerificationOutcome,
    /// Unmodified copied project.
    pub baseline: VerificationCommandReport,
    /// Scratch-only offline lockfile refresh after a successful locked baseline.
    /// None means the baseline failed and candidate preparation was not attempted.
    pub candidate_preparation: Option<VerificationCommandReport>,
    /// Same selected locked compile command and environment after scratch preparation.
    pub candidate_result: VerificationCommandReport,
}

/// Serializable finite verification results, never an instruction to apply a plan.
#[derive(Debug, Clone, Default, Serialize)]
pub struct VerificationReport {
    /// Per-profile baseline/candidate observations.
    pub profiles: Vec<VerificationProfileReport>,
    /// Explicit coverage and environmental limitations.
    pub diagnostics: Vec<String>,
}

/// Verify a sealed, current plan using isolated complete project copies.
///
/// Baseline and candidate get separate copies, empty Cargo homes and dedicated
/// target directories; all compile commands use `--offline --locked`. Candidate
/// preparation explicitly runs `generate-lockfile --offline` only in its disposable
/// copy, and is reported separately; compilation is skipped if preparation fails.
/// No user tree manifest or lockfile is changed. Copies are NOT an OS security sandbox.
///
/// # Errors
/// * Missing deliberate execution consent, empty/invalid profiles, invalid tool paths.
/// * Stale/incomplete/tampered plans, symlinks or filesystem errors.
/// * Local paths outside the copied root, project Cargo config/toolchain ambiguity.
/// * Unsupported process-group supervision platform (only Unix is supported).
///
/// Command failures are returned as observations, not successful verification.
pub fn verify_machete_plan(
    plan: &MachetePlan,
    options: &VerificationOptions,
) -> Result<VerificationReport, BoxError> {
    validate_plan(plan)?;
    verify_candidates(plan, options)
}

/// Verify privately sealed removal proposals, including uncertain source candidates.
///
/// Results describe only the declared finite profiles and manifest scopes. Neither
/// successful compilation nor failure upgrades source evidence or permits global apply.
/// No candidate plan is exposed to callers.
///
/// # Errors
/// * Invalid analysis inputs, stale/tampered snapshots or unsafe project copies.
/// * Missing execution consent or invalid verification policy/toolchain paths.
pub fn verify_machete(
    options: &MacheteOptions,
    policy: &VerificationOptions,
) -> Result<VerificationReport, BoxError> {
    validate_options(policy)?;
    let plan = plan_machete_candidates(options)?;
    verify_candidates(&plan, policy)
}

fn verify_candidates(
    plan: &MachetePlan,
    options: &VerificationOptions,
) -> Result<VerificationReport, BoxError> {
    validate_evidence(plan)?;
    validate_options(options)?;
    if plan.changes.is_empty() {
        return Ok(VerificationReport {
            profiles: Vec::new(),
            diagnostics: vec![
                "No removal proposals; no comparisons, project copies or compiler baselines were performed. No publication is authorized.".into(),
            ],
        });
    }
    let root = plan.report.root.canonicalize()?;
    if root != plan.report.root {
        return Err("verification root must be canonical".into());
    }
    // Retain ignored/generated inputs except explicit artifact/VCS directories.
    validate_pruned_inputs(plan, &root)?;
    let staging = tempfile::tempdir()?;
    let pristine = staging.path().join("pristine");
    copy_project(&root, &pristine, &root)?;
    validate_evidence(plan)?;
    validate_copied_evidence(plan, &root, &pristine)?;
    let scopes = verification_scopes(plan, options);
    let mut report = VerificationReport { profiles: Vec::new(), diagnostics: vec![
        "Finite profiles are not universal dependency-absence or compiler-name-resolution proof. Candidate compilation failure is not usage proof; no verification outcome authorizes global apply.".into(),
        "Copies prune target/.git before traversal and reject sealed inputs under those directories; other ignored/generated inputs are retained.".into(),
        "Temporary copies are not a security sandbox: build scripts and proc macros run with caller privileges.".into(),
        "Empty isolated Cargo home; offline missing dependencies/toolchains are baseline environment failures. Candidate lockfiles are explicitly refreshed offline only in the disposable copy before locked compilation; preparation failures are reported separately and prevent candidate compilation. Individual scope is a manifest replacement, not a single declaration.".into(),
    ] };
    let cargo_identity = bounded_identity(&options.cargo_path)?;
    let rustc_identity = bounded_identity(&options.rustc_path)?;
    report.diagnostics.push(format!(
        "Selected executable identities: cargo SHA-256 {cargo_identity}; rustc SHA-256 {rustc_identity}. These identify executables, not the full sysroot or external input closure."
    ));
    // Profiles differing only in labels or selector ordering compile the same
    // unit group. Reuse the pristine baseline, never candidate build state.
    let mut baselines = BTreeMap::new();
    for profile in &options.profiles {
        if scopes.is_empty() {
            break;
        }
        let args = arguments(profile);
        let key = baseline_key(profile);
        validate_evidence(plan)?;
        let baseline = if let Some(baseline) = baselines.get(&key) {
            VerificationCommandReport::clone(baseline)
        } else {
            let baseline_dir = tempfile::tempdir()?;
            let baseline_project = baseline_dir.path().join("project");
            copy_project(&pristine, &baseline_project, &pristine)?;
            let baseline = execute(&baseline_project, baseline_dir.path(), &args, options)?;
            baselines.insert(key, baseline.clone());
            baseline
        };
        for (scope, indices) in &scopes {
            let candidate_dir = tempfile::tempdir()?;
            let candidate_project = candidate_dir.path().join("project");
            copy_project(&pristine, &candidate_project, &pristine)?;
            for &index in indices {
                let change = &plan.changes[index];
                fs::write(
                    candidate_project.join(change.path.strip_prefix(&root)?),
                    &change.after,
                )?;
            }
            validate_evidence(plan)?;
            let (candidate_preparation, candidate_result) = candidate_commands(
                plan,
                &baseline,
                &candidate_project,
                candidate_dir.path(),
                &args,
                options,
            )?;
            report.profiles.push(VerificationProfileReport {
                profile: profile.clone(),
                candidate: scope.clone(),
                manifests: indices
                    .iter()
                    .map(|&index| plan.changes[index].path.clone())
                    .collect(),
                outcome: match (&baseline.status, &candidate_result.status) {
                    (VerificationStatus::Passed, VerificationStatus::Passed) => {
                        VerificationOutcome::CandidateCompiled
                    }
                    (VerificationStatus::Passed, VerificationStatus::Failed) => {
                        VerificationOutcome::CandidateFailed
                    }
                    _ => VerificationOutcome::Unknown,
                },
                baseline: baseline.clone(),
                candidate_preparation,
                candidate_result,
            });
        }
    }
    validate_evidence(plan)?;
    if bounded_identity(&options.cargo_path)? != cargo_identity
        || bounded_identity(&options.rustc_path)? != rustc_identity
    {
        return Err("toolchain changed during compiler verification".into());
    }
    Ok(report)
}

fn verification_scopes(
    plan: &MachetePlan,
    options: &VerificationOptions,
) -> Vec<(String, Vec<usize>)> {
    let mut scopes = Vec::new();
    if options.combined && !plan.changes.is_empty() {
        scopes.push(("combined".to_owned(), (0..plan.changes.len()).collect()));
    }
    if options.individual {
        scopes.extend(
            plan.changes
                .iter()
                .enumerate()
                .map(|(index, change)| (change.path.display().to_string(), vec![index])),
        );
    }
    scopes
}

/// The only unlocked Cargo operation in verification; always scratch-only.
#[derive(Clone, Copy)]
enum CandidatePreparation {
    RefreshOfflineLockfile,
}
impl CandidatePreparation {
    fn arguments(self) -> Vec<String> {
        match self {
            Self::RefreshOfflineLockfile => vec!["generate-lockfile".into(), "--offline".into()],
        }
    }
}

fn not_run(arguments: &[String], reason: &str) -> VerificationCommandReport {
    VerificationCommandReport {
        arguments: arguments.to_vec(),
        status: VerificationStatus::NotRun,
        exit_code: None,
        stdout: String::new(),
        stderr: reason.into(),
        output_truncated: false,
    }
}

fn candidate_commands(
    plan: &MachetePlan,
    baseline: &VerificationCommandReport,
    project: &Path,
    isolation: &Path,
    arguments: &[String],
    options: &VerificationOptions,
) -> Result<(Option<VerificationCommandReport>, VerificationCommandReport), BoxError> {
    if !matches!(baseline.status, VerificationStatus::Passed) {
        return Ok((
            None,
            not_run(
                arguments,
                "baseline failed; candidate inference unavailable",
            ),
        ));
    }
    validate_evidence(plan)?;
    let preparation = execute(
        project,
        isolation,
        &CandidatePreparation::RefreshOfflineLockfile.arguments(),
        options,
    )?;
    if !matches!(preparation.status, VerificationStatus::Passed) {
        return Ok((
            Some(preparation),
            not_run(
                arguments,
                "scratch offline lockfile preparation failed; candidate compilation not attempted",
            ),
        ));
    }
    validate_evidence(plan)?;
    let candidate = execute(project, isolation, arguments, options)?;
    validate_evidence(plan)?;
    Ok((Some(preparation), candidate))
}

// A skipped artifact directory must never silently erase sealed source or generated
// evidence. Refuse even negative snapshots: absence inside a pruned tree is not
// represented faithfully by a copy which omits that entire tree.
fn validate_pruned_inputs(plan: &MachetePlan, root: &Path) -> Result<(), BoxError> {
    for path in plan.snapshots.keys().chain(plan.directories.keys()) {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        if relative.components().any(|part| {
            let name = part.as_os_str();
            name == "target" || name == ".git"
        }) {
            return Err(format!(
                "verification refuses required input under pruned target/.git directory: {}",
                path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn validate_copied_evidence(
    plan: &MachetePlan,
    root: &Path,
    pristine: &Path,
) -> Result<(), BoxError> {
    for (path, expected) in &plan.snapshots {
        if let Ok(relative) = path.strip_prefix(root) {
            let current = match fs::read(pristine.join(relative)) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            if &current != expected {
                return Err(format!(
                    "copied evidence differs from sealed plan: {}",
                    path.display()
                )
                .into());
            }
        }
    }
    for change in &plan.changes {
        if fs::read(pristine.join(change.path.strip_prefix(root)?))? != change.before.as_bytes() {
            return Err("copied manifest differs from sealed plan".into());
        }
    }
    Ok(())
}

fn validate_options(options: &VerificationOptions) -> Result<(), BoxError> {
    if !options.allow_execution {
        return Err("compiler verification requires explicit allow_execution (build scripts/proc macros may execute)".into());
    }
    if !cfg!(unix) {
        return Err("verification process-group supervision requires Unix".into());
    }
    if options.profiles.is_empty()
        || (!options.combined && !options.individual)
        || options.timeout_ms == 0
        || options.max_output_bytes == 0
        || options.max_output_bytes > 16 * 1024 * 1024
    {
        return Err("verification requires profiles, a candidate scope, positive timeout and 1..=16 MiB output limit".into());
    }
    for (path, name) in [
        (&options.cargo_path, "cargo"),
        (&options.rustc_path, "rustc"),
    ] {
        if !path.is_absolute()
            || !path.is_file()
            || path.canonicalize()? != *path
            || path.file_name().is_none_or(|file| file != name)
        {
            return Err(format!(
                "select an absolute canonical installed {name} binary (not rustup proxy)"
            )
            .into());
        }
    }
    if let Some(linker) = &options.linker_path {
        if !linker.is_absolute() || !linker.is_file() || linker.canonicalize()? != *linker {
            return Err("select an absolute canonical installed linker".into());
        }
        if linker
            .to_str()
            .is_none_or(|value| value.contains('\u{001f}'))
        {
            return Err("linker path must be UTF-8 without the Cargo flag separator".into());
        }
    }
    if options.cargo_path.parent() != options.rustc_path.parent() {
        return Err("Cargo and rustc must belong to the same installed toolchain directory".into());
    }
    let mut names = std::collections::BTreeSet::new();
    for profile in &options.profiles {
        if profile.name.trim().is_empty()
            || !names.insert(&profile.name)
            || (profile.all_features && !profile.features.is_empty())
        {
            return Err(
                "profile names must be nonempty/unique and feature selectors compatible".into(),
            );
        }
        for value in profile
            .packages
            .iter()
            .chain(&profile.features)
            .chain(profile.target.iter())
        {
            if value.is_empty()
                || !value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-/.:".contains(&c))
                || value.starts_with('-')
            {
                return Err("invalid Cargo selector".into());
            }
        }
    }
    Ok(())
}

/// Explicit selected-toolchain expansion request. No automatic nightly selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpansionOptions {
    /// Execution consent, installed toolchain binaries, timeout and output policy.
    pub verification: VerificationOptions,
    /// One library configuration (all-targets is not supported for expansion).
    pub profile: VerificationProfile,
    /// Exact package name, required for workspaces with multiple library packages.
    pub package: Option<String>,
}

/// Bounded compiler-specific expansion transport and generation-time provenance.
#[derive(Debug, Clone, Serialize)]
pub struct ExpansionReport {
    /// Exact configuration; finite and compiler-specific.
    pub profile: VerificationProfile,
    /// Caller-selected installed Cargo binary (never automatically replaced).
    pub cargo_path: PathBuf,
    /// Caller-selected installed rustc binary (never automatically replaced).
    pub rustc_path: PathBuf,
    /// Lowercase SHA-256 of the selected Cargo binary (streamed, at most 128 MiB).
    pub cargo_identity: String,
    /// Lowercase SHA-256 of the selected rustc binary (streamed, at most 128 MiB).
    pub rustc_identity: String,
    /// Root-relative project inputs snapshotted before execution.
    pub inputs: Vec<super::generated::InputFingerprint>,
    /// Command status and bounded output. Stable rejection is an explicit failure.
    pub command: VerificationCommandReport,
    /// Expanded Rust only on successful, untruncated output; no global absence proof.
    pub expanded_rust: Option<String>,
    /// Capability and trust limitations, including actionable unsupported diagnostics.
    pub diagnostics: Vec<String>,
}

/// Expand one library using an explicitly selected installed nightly-capable compiler.
///
/// Executes `cargo rustc --offline --locked --lib ... -- -Zunpretty=expanded` in a
/// disposable complete copy. It does not set `RUSTC_BOOTSTRAP`, install toolchains,
/// choose nightly, run rustup, or mutate the source tree. Stable compilers reject
/// the unstable flag and their diagnostic is preserved as an unsuccessful report.
/// This is compiler-specific macro expansion, not name resolution or absence proof.
///
/// # Errors
/// * Missing execution consent or invalid toolchain/profile/options.
/// * Unsafe copies/configuration/paths, excessive input/output identity size or I/O errors.
/// * Input bytes changed while preparing the copy.
///
/// Compiler/environment failures are retained in the returned command observation.
pub fn expand_machete_project(
    root: &Path,
    options: &ExpansionOptions,
) -> Result<ExpansionReport, BoxError> {
    let mut policy = options.verification.clone();
    policy.profiles = vec![options.profile.clone()];
    validate_options(&policy)?;
    if options.profile.all_targets
        || !matches!(options.profile.operation, VerificationOperation::Check)
    {
        return Err(
            "expansion selects one library with operation CHECK; all-targets is unsupported".into(),
        );
    }
    let mut profile = options.profile.clone();
    if let Some(package) = &options.package {
        if !profile.packages.is_empty() && profile.packages != [package.clone()] {
            return Err("conflicting expansion package selectors".into());
        }
        profile.packages = vec![package.clone()];
        policy.profiles = vec![profile.clone()];
        validate_options(&policy)?;
    }
    if profile.packages.len() > 1 {
        return Err("expansion requires a single library package".into());
    }
    if matches!(profile.operation, VerificationOperation::TestNoRun) {
        return Err("library expansion does not support test-no-run profiles".into());
    }
    let root = root.canonicalize()?;
    let isolation = tempfile::tempdir()?;
    let project = isolation.path().join("project");
    copy_project(&root, &project, &root)?;
    let mut inputs = Vec::new();
    fingerprint_copy(&project, &project, &mut inputs, &mut 0)?;
    // Validate generation-time provenance against the source before executing.
    for input in &inputs {
        if fs::read(root.join(&input.path))? != input.bytes {
            return Err("project changed during expansion capture".into());
        }
    }
    let cargo_identity = bounded_identity(&policy.cargo_path)?;
    let rustc_identity = bounded_identity(&policy.rustc_path)?;
    let mut args = arguments(&profile);
    args[0] = "rustc".into();
    args.extend(["--lib".into(), "--".into(), "-Zunpretty=expanded".into()]);
    let command = execute(&project, isolation.path(), &args, &policy)?;
    validate_capture_inputs(&root, &inputs)?;
    if bounded_identity(&policy.cargo_path)? != cargo_identity
        || bounded_identity(&policy.rustc_path)? != rustc_identity
    {
        return Err("toolchain changed during expansion capture".into());
    }
    let expanded_rust = if matches!(command.status, VerificationStatus::Passed)
        && !command.output_truncated
        && !command.stdout.trim().is_empty()
        && syn::parse_file(&command.stdout).is_ok()
    {
        Some(command.stdout.clone())
    } else {
        None
    };
    let mut diagnostics = vec!["Expansion is compiler-specific positive evidence for one library/profile only. Generation snapshots cover copied files, not undeclared environment/system inputs. Temporary copies are not a security sandbox.".into()];
    if expanded_rust.is_none() {
        diagnostics.push("Expansion unavailable: explicitly select an already installed nightly-capable Cargo/rustc pair and a locally available offline dependency set; stable rustc does not support -Zunpretty=expanded. No toolchain was selected, installed or bootstrapped automatically. Check command status/output; raise the bounded output limit for truncated output.".into());
    }
    Ok(ExpansionReport {
        profile,
        cargo_path: policy.cargo_path,
        rustc_path: policy.rustc_path,
        cargo_identity,
        rustc_identity,
        inputs,
        command,
        expanded_rust,
        diagnostics,
    })
}

/// Revalidate a library expansion capture against the selected profile and tools.
///
/// This performs file I/O only: no Cargo, compiler, macro or build script executes.
/// Passing establishes declared copied-file freshness and tool executable identity,
/// not semantic resolution, input closure, or permission to remove dependencies.
///
/// # Errors
/// * Profile/tool mismatch, failed/truncated/empty capture or malformed expanded Rust.
/// * Changed, added or removed copied inputs; unsafe paths or copy/identity errors.
pub fn validate_expansion_report(
    root: &Path,
    report: &ExpansionReport,
    expected_profile: &VerificationProfile,
    cargo_path: &Path,
    rustc_path: &Path,
) -> Result<(), BoxError> {
    if serde_json::to_vec(&report.profile)? != serde_json::to_vec(expected_profile)?
        || report.cargo_path != cargo_path
        || report.rustc_path != rustc_path
        || bounded_identity(cargo_path)? != report.cargo_identity
        || bounded_identity(rustc_path)? != report.rustc_identity
    {
        return Err("expansion capture profile/toolchain mismatch".into());
    }
    let mut expected_arguments = arguments(expected_profile);
    expected_arguments[0] = "rustc".into();
    expected_arguments.extend(["--lib".into(), "--".into(), "-Zunpretty=expanded".into()]);
    if report.command.arguments != expected_arguments {
        return Err("expansion command differs from the selected library profile".into());
    }
    if !matches!(report.command.status, VerificationStatus::Passed)
        || report.command.output_truncated
    {
        return Err("expansion capture is failed or truncated".into());
    }
    let text = report
        .expanded_rust
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .ok_or("expansion capture has no complete output")?;
    if text != report.command.stdout {
        return Err("expansion text differs from captured compiler output".into());
    }
    syn::parse_file(text)?;
    validate_capture_inputs(&root.canonicalize()?, &report.inputs)
}

fn validate_capture_inputs(
    root: &Path,
    expected: &[super::generated::InputFingerprint],
) -> Result<(), BoxError> {
    // Reapply the copy policy without copying bytes or executing producers.
    // Additions/deletions matter; pruned artifact trees deliberately do not.
    let mut actual = Vec::new();
    fingerprint_copy(root, root, &mut actual, &mut 0)?;
    if actual.len() != expected.len()
        || actual.iter().zip(expected).any(|(actual, expected)| {
            actual.path != expected.path || actual.bytes != expected.bytes
        })
    {
        return Err("expansion capture copied input inventory changed".into());
    }
    Ok(())
}

fn bounded_identity(path: &Path) -> Result<String, BoxError> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    const MAX_BYTES: u64 = 128 * 1024 * 1024;
    let mut reader = fs::File::open(path)?.take(MAX_BYTES + 1);
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_BYTES {
            return Err("toolchain executable exceeds identity size limit".into());
        }
        digest.update(&buffer[..count]);
    }
    let mut identity = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(identity, "{byte:02x}")?;
    }
    Ok(identity)
}

fn fingerprint_copy(
    root: &Path,
    directory: &Path,
    result: &mut Vec<super::generated::InputFingerprint>,
    total: &mut usize,
) -> Result<(), BoxError> {
    let mut entries = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    for path in entries {
        let name = path.file_name().ok_or("input filename missing")?;
        if name == "target" || name == ".git" {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            return Err("expansion provenance rejects symlinks and non-regular inputs".into());
        }
        if name == ".cargo" || name == "rust-toolchain" || name == "rust-toolchain.toml" {
            return Err(
                "expansion provenance refuses ambiguous Cargo/toolchain configuration".into(),
            );
        }
        if metadata.is_dir() {
            fingerprint_copy(root, &path, result, total)?;
        } else {
            if name == "Cargo.toml" {
                validate_manifest(&path, root)?;
            }
            let size = usize::try_from(metadata.len())?;
            if size > 16 * 1024 * 1024 || total.saturating_add(size) > 64 * 1024 * 1024 {
                return Err("expansion provenance exceeds 16 MiB per input or 64 MiB total".into());
            }
            let mut bytes = Vec::new();
            fs::File::open(&path)?
                .take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 16 * 1024 * 1024
                || total.saturating_add(bytes.len()) > 64 * 1024 * 1024
            {
                return Err("expansion provenance grew beyond capture size limits".into());
            }
            *total += bytes.len();
            result.push(super::generated::InputFingerprint {
                path: path.strip_prefix(root)?.to_owned(),
                bytes,
            });
        }
    }
    Ok(())
}

fn baseline_key(profile: &VerificationProfile) -> Vec<String> {
    let mut canonical = profile.clone();
    canonical.packages.sort();
    canonical.packages.dedup();
    canonical.features.sort();
    canonical.features.dedup();
    arguments(&canonical)
}

fn arguments(profile: &VerificationProfile) -> Vec<String> {
    let mut args = vec![
        match profile.operation {
            VerificationOperation::Check => "check",
            VerificationOperation::TestNoRun => "test",
            VerificationOperation::Build => "build",
        }
        .into(),
        "--offline".into(),
        "--locked".into(),
    ];
    if matches!(profile.operation, VerificationOperation::TestNoRun) {
        args.push("--no-run".into());
    }
    if profile.no_default_features {
        args.push("--no-default-features".into());
    }
    if profile.all_features {
        args.push("--all-features".into());
    }
    if profile.all_targets {
        args.push("--all-targets".into());
    }
    if profile.release {
        args.push("--release".into());
    }
    for package in &profile.packages {
        args.extend(["--package".into(), package.clone()]);
    }
    for feature in &profile.features {
        args.extend(["--features".into(), feature.clone()]);
    }
    if let Some(target) = &profile.target {
        args.extend(["--target".into(), target.clone()]);
    }
    args
}

// Reject configuration rather than trying to partially interpret its ability to
// redirect targets, runners, rustc wrappers, credential helpers or source caches.
fn copy_project(source: &Path, destination: &Path, root: &Path) -> Result<(), BoxError> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        // Prune before metadata/traversal: artifacts can contain huge trees,
        // stale symlinks, sockets, and unreadable files irrelevant to this copy.
        if name == "target" || name == ".git" {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(format!("verification rejects symlink {}", path.display()).into());
        }
        if name == ".cargo" || name == "rust-toolchain" || name == "rust-toolchain.toml" {
            return Err(format!(
                "verification refuses ambiguous Cargo/toolchain configuration: {}",
                path.display()
            )
            .into());
        }
        let output = destination.join(name);
        if metadata.is_dir() {
            copy_project(&path, &output, root)?;
        } else if metadata.is_file() {
            if path.file_name().is_some_and(|name| name == "Cargo.toml") {
                validate_manifest(&path, root)?;
            }
            fs::copy(&path, output)?;
        } else {
            return Err("verification rejects non-regular files".into());
        }
    }
    Ok(())
}

fn validate_manifest(path: &Path, root: &Path) -> Result<(), BoxError> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(path)?)?;
    inspect_paths(
        &value,
        path.parent().ok_or("manifest parent missing")?,
        root,
        None,
    )
}
fn inspect_paths(
    value: &toml::Value,
    parent: &Path,
    root: &Path,
    key: Option<&str>,
) -> Result<(), BoxError> {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                inspect_paths(value, parent, root, Some(key))?;
            }
        }
        toml::Value::Array(array) => {
            for value in array {
                inspect_paths(value, parent, root, key)?;
            }
        }
        toml::Value::String(value)
            if matches!(
                key,
                Some(
                    "path"
                        | "workspace"
                        | "readme"
                        | "license-file"
                        | "build"
                        | "members"
                        | "exclude"
                        | "default-members"
                )
            ) =>
        {
            let relative = Path::new(value);
            if relative.is_absolute() {
                return Err(format!("absolute manifest path is unsafe: {value}").into());
            }
            let mut resolved = parent.to_owned();
            for component in relative.components() {
                match component {
                    std::path::Component::ParentDir => {
                        resolved.pop();
                    }
                    std::path::Component::Normal(part) => resolved.push(part),
                    std::path::Component::CurDir => (),
                    _ => return Err("unsupported manifest path".into()),
                }
                if !resolved.starts_with(root) {
                    return Err(format!("manifest path escapes copied project: {value}").into());
                }
            }
        }
        _ => (),
    }
    Ok(())
}

#[cfg(unix)]
fn isolated_command(
    project: &Path,
    isolation: &Path,
    args: &[String],
    options: &VerificationOptions,
) -> Result<Command, BoxError> {
    use std::os::unix::process::CommandExt;
    let tool_directory = options.rustc_path.parent().ok_or("rustc has no parent")?;
    let mut command = Command::new(&options.cargo_path);
    command
        .args(args)
        .current_dir(project)
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", tool_directory.display()),
        )
        .env("HOME", isolation.join("home"))
        .env("CARGO_HOME", isolation.join("cargo-home"))
        .env("CARGO_TARGET_DIR", isolation.join("target"))
        .env("TMPDIR", isolation.join("tmp"))
        .env("RUSTC", &options.rustc_path)
        .env("CARGO_NET_OFFLINE", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(linker) = &options.linker_path {
        command.env(
            "CARGO_ENCODED_RUSTFLAGS",
            format!("-Clinker={}", linker.display()),
        );
    }
    Ok(command)
}

#[cfg(unix)]
fn execute(
    project: &Path,
    isolation: &Path,
    args: &[String],
    options: &VerificationOptions,
) -> Result<VerificationCommandReport, BoxError> {
    reject_ancestor_configuration(project)?;
    let home = isolation.join("home");
    let cargo_home = isolation.join("cargo-home");
    let target = isolation.join("target");
    let tmp = isolation.join("tmp");
    for path in [&home, &cargo_home, &target, &tmp] {
        fs::create_dir_all(path)?;
    }
    let mut report = VerificationCommandReport {
        arguments: args.to_vec(),
        status: VerificationStatus::EnvironmentFailure,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
    };
    let mut command = isolated_command(project, isolation, args, options)?;
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(options.timeout_ms))
        .ok_or("verification deadline overflows monotonic clock")?;
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            report.stderr = error.to_string();
            return Ok(report);
        }
    };
    let stdout = child.stdout.take().ok_or("missing child stdout")?;
    let stderr = child.stderr.take().ok_or("missing child stderr")?;
    let out = capture(stdout, options.max_output_bytes);
    let err = capture(stderr, options.max_output_bytes);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                report.exit_code = status.code();
                report.status = if status.success() {
                    VerificationStatus::Passed
                } else {
                    VerificationStatus::Failed
                };
                break;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                report.status = VerificationStatus::TimedOut;
                break;
            }
            Err(error) => {
                report.stderr = error.to_string();
                break;
            }
        }
    }
    // Fixed infrastructure script: only the numeric process-group ID is passed
    // as data. Use POSIX `-s KILL --`: dash rejects `-KILL --`, while the
    // builtin also works on Nix without /bin/kill.
    let cleanup = Command::new("/bin/sh")
        .env_clear()
        .args([
            "-c",
            "kill -s KILL -- \"$1\"",
            "clippier-process-cleanup",
            &format!("-{}", child.id()),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if cleanup.is_err() {
        report.status = VerificationStatus::EnvironmentFailure;
    }
    let _ = child.kill();
    let _ = child.wait();
    for (receiver, destination) in [(out, &mut report.stdout), (err, &mut report.stderr)] {
        match receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok((bytes, truncated))) => {
                *destination = String::from_utf8_lossy(&bytes).into_owned();
                report.output_truncated |= truncated;
            }
            Ok(Err(error)) => {
                *destination = error.to_string();
                report.status = VerificationStatus::EnvironmentFailure;
            }
            Err(error) => {
                *destination = format!("output capture failed: {error}");
                report.status = VerificationStatus::EnvironmentFailure;
            }
        }
    }
    Ok(report)
}

fn reject_ancestor_configuration(project: &Path) -> Result<(), BoxError> {
    for ancestor in project.ancestors().skip(1) {
        if ancestor.join(".cargo").exists() || ancestor.join("Cargo.toml").exists() {
            return Err(format!(
                "temporary project has ambiguous ancestor Cargo configuration: {}",
                ancestor.display()
            )
            .into());
        }
    }
    Ok(())
}

fn capture(
    mut reader: impl Read + Send + 'static,
    maximum: usize,
) -> std::sync::mpsc::Receiver<std::io::Result<(Vec<u8>, bool)>> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (|| {
            let mut bytes = Vec::new();
            let mut truncated = false;
            let mut buffer = [0; 8192];
            loop {
                let count = reader.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                let retain = count.min(maximum.saturating_sub(bytes.len()));
                bytes.extend_from_slice(&buffer[..retain]);
                truncated |= retain < count;
            }
            Ok((bytes, truncated))
        })();
        let _ = sender.send(result);
    });
    receiver
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn toolchain(root: &Path, script: &str) -> VerificationOptions {
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        for (name, contents) in [("cargo", script), ("rustc", "#!/bin/sh\nexit 0\n")] {
            let path = bin.join(name);
            fs::write(&path, contents).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        VerificationOptions {
            allow_execution: true,
            cargo_path: bin.join("cargo"),
            rustc_path: bin.join("rustc"),
            profiles: vec![VerificationProfile {
                name: "fixture".into(),
                ..VerificationProfile::default()
            }],
            ..VerificationOptions::default()
        }
    }

    #[test]
    fn expansion_transport_is_typed_offline_locked_and_fingerprinted() {
        let tool = tempfile::tempdir().unwrap();
        let policy = toolchain(
            tool.path(),
            "#!/bin/sh\n[ \"$1\" = rustc ] && [ \"$2\" = --offline ] && [ \"$3\" = --locked ] && [ \"$4\" = --lib ] && [ \"$5\" = -- ] && [ \"$6\" = -Zunpretty=expanded ] || exit 9\n[ -z \"$RUSTC_BOOTSTRAP\" ] && [ -z \"$RUSTC_WRAPPER\" ] || exit 8\nprintf 'pub fn expanded() { alias::call(); }\\n'\n",
        );
        let source = tempfile::tempdir().unwrap();
        fs::write(
            source.path().join("Cargo.toml"),
            "[package]\nname='expansion'\nversion='0.1.0'\n",
        )
        .unwrap();
        let options = ExpansionOptions {
            profile: policy.profiles[0].clone(),
            verification: policy,
            package: None,
        };
        let report = expand_machete_project(source.path(), &options).unwrap();
        assert!(report.expanded_rust.unwrap().contains("alias::call"));
        assert_eq!(report.inputs.len(), 1);
        assert_eq!(report.inputs[0].path, Path::new("Cargo.toml"));
        assert!(!source.path().join("target").exists());
        for identity in [&report.cargo_identity, &report.rustc_identity] {
            assert_eq!(identity.len(), 64);
            assert!(
                identity
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            );
        }
        assert_eq!(
            report.cargo_identity,
            bounded_identity(&options.verification.cargo_path).unwrap()
        );
        assert_eq!(
            report.rustc_identity,
            bounded_identity(&options.verification.rustc_path).unwrap()
        );
    }

    #[test]
    fn executable_identity_is_streamed_sha256_and_rejects_oversized_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("identity");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            bounded_identity(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let file = fs::File::create(&path).unwrap();
        file.set_len(128 * 1024 * 1024 + 1).unwrap();
        assert!(
            bounded_identity(&path)
                .unwrap_err()
                .to_string()
                .contains("identity size limit")
        );
    }

    #[test]
    fn deadline_and_output_limits_are_observations_not_proof() {
        let tool = tempfile::tempdir().unwrap();
        let mut options = toolchain(
            tool.path(),
            "#!/bin/sh\nprintf '0123456789'\nwhile :; do :; done\n",
        );
        options.timeout_ms = 30;
        options.max_output_bytes = 3;
        let isolation = tempfile::tempdir().unwrap();
        let project = isolation.path().join("project");
        fs::create_dir(&project).unwrap();
        let report = execute(&project, isolation.path(), &[], &options).unwrap();
        assert!(matches!(report.status, VerificationStatus::TimedOut));
        assert_eq!(report.stdout, "012");
        assert!(report.output_truncated);
    }

    fn plan_fixture() -> (tempfile::TempDir, MachetePlan) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(root.path().join("Cargo.toml"), "[package]\nname='verification'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nunused='1.0.0'\n").unwrap();
        fs::write(root.path().join("src/lib.rs"), "pub fn library() {}\n").unwrap();
        let plan = super::super::plan_machete(&super::super::MacheteOptions {
            manifest_path: Some(root.path().join("Cargo.toml")),
            ..super::super::MacheteOptions::default()
        })
        .unwrap();
        assert_eq!(plan.changes.len(), 1);
        (root, plan)
    }

    #[test]
    fn no_proposals_skip_project_copy_and_all_compiler_commands() {
        let tool = tempfile::tempdir().unwrap();
        let marker = tool.path().join("executed");
        let policy = toolchain(
            tool.path(),
            &format!("#!/bin/sh\ntouch '{}'\nexit 0\n", marker.display()),
        );
        let (root, _) = plan_fixture();
        fs::write(root.path().join("src/lib.rs"), "pub use unused::Thing;\n").unwrap();
        // Copying would reject this directory, so success also proves no copy.
        fs::create_dir(root.path().join(".cargo")).unwrap();
        let options = MacheteOptions {
            manifest_path: Some(root.path().to_owned()),
            ..Default::default()
        };
        let plan = super::super::plan_machete(&options).unwrap();
        assert!(plan.changes.is_empty());
        for report in [
            verify_machete_plan(&plan, &policy).unwrap(),
            verify_machete(&options, &policy).unwrap(),
        ] {
            assert!(report.profiles.is_empty());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|message| message.contains("no comparisons"))
            );
        }
        assert!(!marker.exists());
        assert!(!root.path().join("target").exists());
    }

    #[test]
    fn uncertain_candidates_are_verified_without_global_apply_authority() {
        let tool = tempfile::tempdir().unwrap();
        let policy = toolchain(tool.path(), "#!/bin/sh\nexit 0\n");
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(root.path().join("Cargo.toml"), "[package]\nname='verification'\nversion='0.1.0'\nedition='2021'\n[dependencies]\nunused='1.0.0'\n").unwrap();
        fs::write(root.path().join("src/lib.rs"), "opaque!();\n").unwrap();
        let options = MacheteOptions {
            manifest_path: Some(root.path().to_owned()),
            ..Default::default()
        };
        let public_plan = super::super::plan_machete(&options).unwrap();
        assert!(!public_plan.report.complete);
        assert!(public_plan.changes.is_empty());
        assert!(verify_machete_plan(&public_plan, &policy).is_err());
        assert!(super::super::apply_machete_plan(&public_plan).is_err());
        let private_plan = plan_machete_candidates(&options).unwrap();
        assert!(!private_plan.complete);
        assert_eq!(private_plan.changes.len(), 1);
        assert!(super::super::apply_machete_plan(&private_plan).is_err());
        let before = fs::read(root.path().join("Cargo.toml")).unwrap();
        let report = verify_machete(&options, &policy).unwrap();
        assert!(matches!(
            report.profiles[0].outcome,
            VerificationOutcome::CandidateCompiled
        ));
        assert_eq!(fs::read(root.path().join("Cargo.toml")).unwrap(), before);
        assert!(
            !super::super::plan_machete(&options)
                .unwrap()
                .report
                .complete
        );
    }

    #[test]
    fn baselines_are_reused_per_equivalent_unit_group_but_not_between_calls() {
        let tool = tempfile::tempdir().unwrap();
        let log = tool.path().join("commands");
        let mut options = toolchain(
            tool.path(),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\nexit 0\n",
                log.display()
            ),
        );
        options.individual = true;
        options.profiles.push(VerificationProfile {
            name: "second".into(),
            ..Default::default()
        });
        let (_root, plan) = plan_fixture();
        for expected in [9, 18] {
            let report = verify_machete_plan(&plan, &options).unwrap();
            assert_eq!(report.profiles.len(), 4);
            // Equivalent profile labels share one baseline, with four preparations
            // and four independent candidates. A new call must rerun its baseline.
            assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), expected);
            assert!(
                report.profiles.iter().all(|profile| matches!(
                    profile.outcome,
                    VerificationOutcome::CandidateCompiled
                ))
            );
            assert!(
                report
                    .profiles
                    .iter()
                    .all(|profile| profile.manifests == [plan.changes[0].path.clone()])
            );
        }
    }

    #[test]
    fn artifact_trees_are_pruned_without_traversing_but_generated_inputs_survive() {
        let root = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        for directory in ["target/debug", ".git/objects", "generated"] {
            fs::create_dir_all(root.path().join(directory)).unwrap();
        }
        for directory in ["target/debug", ".git/objects"] {
            std::os::unix::fs::symlink("/missing", root.path().join(directory).join("unreadable"))
                .unwrap();
        }
        fs::write(
            root.path().join("generated/source.rs"),
            "pub fn generated() {}\n",
        )
        .unwrap();
        let copy = output.path().join("project");
        copy_project(root.path(), &copy, root.path()).unwrap();
        assert!(!copy.join("target").exists());
        assert!(!copy.join(".git").exists());
        assert_eq!(
            fs::read(copy.join("generated/source.rs")).unwrap(),
            fs::read(root.path().join("generated/source.rs")).unwrap()
        );
    }

    #[test]
    fn required_pruned_inputs_fail_instead_of_silently_disappearing() {
        let (root, mut plan) = plan_fixture();
        for directory in ["target", ".git"] {
            let path = root.path().join(directory).join("generated.rs");
            plan.snapshots
                .insert(path.clone(), Some(b"generated".to_vec()));
            assert!(
                validate_pruned_inputs(&plan, root.path())
                    .unwrap_err()
                    .to_string()
                    .contains("required input")
            );
            plan.snapshots.remove(&path);
            plan.snapshots.insert(path.clone(), None);
            assert!(validate_pruned_inputs(&plan, root.path()).is_err());
            plan.snapshots.remove(&path);
        }
        let path = root.path().join("generated.rs");
        plan.snapshots.insert(path, Some(b"generated".to_vec()));
        assert!(validate_pruned_inputs(&plan, root.path()).is_ok());
    }

    #[test]
    fn baseline_candidate_success_failure_and_not_run_are_distinguished() {
        for (script, expected_baseline, expected_candidate) in [
            ("#!/bin/sh\nexit 0\n", "PASSED", "PASSED"),
            (
                "#!/bin/sh\n[ \"$1\" = generate-lockfile ] && exit 0\nfound=false\nwhile IFS= read -r line; do case \"$line\" in *unused*) found=true;; esac; done < Cargo.toml\n[ \"$found\" = true ] && exit 0\nexit 7\n",
                "PASSED",
                "FAILED",
            ),
            ("#!/bin/sh\nexit 6\n", "FAILED", "NOT_RUN"),
        ] {
            let tool = tempfile::tempdir().unwrap();
            let options = toolchain(tool.path(), script);
            let (root, plan) = plan_fixture();
            let before = fs::read(root.path().join("Cargo.toml")).unwrap();
            let report = verify_machete_plan(&plan, &options).unwrap();
            assert_eq!(report.profiles.len(), 1);
            let profile = &report.profiles[0];
            assert_eq!(
                serde_json::to_value(&profile.baseline.status).unwrap(),
                expected_baseline
            );
            assert_eq!(
                serde_json::to_value(&profile.candidate_result.status).unwrap(),
                expected_candidate
            );
            assert_eq!(
                serde_json::to_value(profile.outcome).unwrap(),
                match expected_candidate {
                    "PASSED" => "CANDIDATE_COMPILED",
                    "FAILED" => "CANDIDATE_FAILED",
                    _ => "UNKNOWN",
                }
            );
            assert_eq!(fs::read(root.path().join("Cargo.toml")).unwrap(), before);
            assert!(!root.path().join("target").exists());
            assert!(!root.path().join("Cargo.lock").exists());
        }
    }

    #[test]
    fn scratch_preparation_is_offline_shared_and_failures_skip_compile() {
        for fail_preparation in [false, true] {
            let tool = tempfile::tempdir().unwrap();
            let options = toolchain(
                tool.path(),
                &format!(
                    "#!/bin/sh\nif [ \"$1\" = generate-lockfile ]; then\n [ \"$2\" = --offline ] && [ \"$#\" = 2 ] || exit 8\n {}\n printf scratch-lock > Cargo.lock\n printf ready > \"$CARGO_HOME/prepared\"\n exit 0\nfi\n[ \"$1\" = check ] && [ \"$2\" = --offline ] && [ \"$3\" = --locked ] || exit 7\nfound=false\nwhile IFS= read -r line; do case \"$line\" in *unused*) found=true;; esac; done < Cargo.toml\nif [ \"$found\" = false ]; then [ -f \"$CARGO_HOME/prepared\" ] && [ -f Cargo.lock ] || exit 6; fi\nexit 0\n",
                    if fail_preparation { "exit 5" } else { ":" }
                ),
            );
            let (root, plan) = plan_fixture();
            let report = verify_machete_plan(&plan, &options).unwrap();
            let observed = &report.profiles[0];
            assert!(matches!(
                observed.baseline.status,
                VerificationStatus::Passed
            ));
            let preparation = observed.candidate_preparation.as_ref().unwrap();
            assert_eq!(preparation.arguments, ["generate-lockfile", "--offline"]);
            if fail_preparation {
                assert!(matches!(preparation.status, VerificationStatus::Failed));
                assert!(matches!(
                    observed.candidate_result.status,
                    VerificationStatus::NotRun
                ));
            } else {
                assert!(matches!(preparation.status, VerificationStatus::Passed));
                assert!(matches!(
                    observed.candidate_result.status,
                    VerificationStatus::Passed
                ));
            }
            assert!(!root.path().join("Cargo.lock").exists());
        }
    }

    #[test]
    fn execution_is_copied_and_environment_is_scrubbed() {
        let tool = tempfile::tempdir().unwrap();
        let options = toolchain(
            tool.path(),
            "#!/bin/sh\nif [ \"$1\" = generate-lockfile ]; then [ \"$2\" = --offline ] || exit 9; printf prepared > \"$CARGO_HOME/prepared\"; exit 0; fi\n[ \"$1\" = check ] && [ \"$2\" = --offline ] && [ \"$3\" = --locked ] || exit 9\n[ -z \"$RUSTC_WRAPPER\" ] && [ -z \"$RUSTFLAGS\" ] || exit 8\n[ -d \"$CARGO_HOME\" ] && [ -d \"$CARGO_TARGET_DIR\" ] && [ -d \"$HOME\" ] || exit 7\nprintf changed > Cargo.toml\nprintf lock > Cargo.lock\nprintf build > \"$CARGO_TARGET_DIR/artifact\"\n",
        );
        let (root, plan) = plan_fixture();
        let before = fs::read(root.path().join("Cargo.toml")).unwrap();
        let report = verify_machete_plan(&plan, &options).unwrap();
        assert!(matches!(
            report.profiles[0].baseline.status,
            VerificationStatus::Passed
        ));
        assert!(matches!(
            report.profiles[0].candidate_result.status,
            VerificationStatus::Passed
        ));
        assert_eq!(fs::read(root.path().join("Cargo.toml")).unwrap(), before);
        assert!(!root.path().join("Cargo.lock").exists());
    }

    #[test]
    fn stale_and_tampered_plans_fail_before_any_command() {
        let tool = tempfile::tempdir().unwrap();
        let marker = tool.path().join("executed");
        let options = toolchain(
            tool.path(),
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        );
        let (root, mut plan) = plan_fixture();
        plan.changes[0].after.push_str("# tampered\n");
        assert!(verify_machete_plan(&plan, &options).is_err());
        assert!(!marker.exists());
        let (_, plan) = plan_fixture();
        fs::write(root.path().join("src/lib.rs"), "changed").unwrap();
        // The dropped fixture above is stale through deletion as well.
        assert!(verify_machete_plan(&plan, &options).is_err());
        let (root, plan) = plan_fixture();
        fs::write(root.path().join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        assert!(verify_machete_plan(&plan, &options).is_err());
        assert!(!marker.exists());
    }

    #[test]
    fn project_configuration_and_symlinks_are_rejected_without_execution() {
        let tool = tempfile::tempdir().unwrap();
        let marker = tool.path().join("executed");
        let options = toolchain(
            tool.path(),
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        );
        for name in [".cargo", "rust-toolchain", "rust-toolchain.toml", "linked"] {
            let (root, _) = plan_fixture();
            if name == ".cargo" {
                fs::create_dir(root.path().join(name)).unwrap();
            } else if name == "linked" {
                std::os::unix::fs::symlink("src/lib.rs", root.path().join(name)).unwrap();
            } else {
                fs::write(root.path().join(name), "nightly").unwrap();
            }
            // Use expansion to inspect a full project without discovery rejecting the fixture first.
            let expanded = expand_machete_project(
                root.path(),
                &ExpansionOptions {
                    profile: options.profiles[0].clone(),
                    verification: options.clone(),
                    package: None,
                },
            );
            assert!(expanded.is_err(), "{name}");
            assert!(!marker.exists());
        }
    }

    #[test]
    fn selected_profile_arguments_are_declared_and_injection_is_rejected() {
        let profile = VerificationProfile {
            name: "coverage".into(),
            operation: VerificationOperation::TestNoRun,
            packages: vec!["fixture".into()],
            features: vec!["optional".into()],
            no_default_features: true,
            all_targets: true,
            release: true,
            target: Some("x86_64-unknown-linux-gnu".into()),
            ..VerificationProfile::default()
        };
        assert_eq!(
            arguments(&profile),
            [
                "test",
                "--offline",
                "--locked",
                "--no-run",
                "--no-default-features",
                "--all-targets",
                "--release",
                "--package",
                "fixture",
                "--features",
                "optional",
                "--target",
                "x86_64-unknown-linux-gnu"
            ]
        );
        let tool = tempfile::tempdir().unwrap();
        let mut options = toolchain(tool.path(), "#!/bin/sh\nexit 0\n");
        options.profiles[0].features = vec!["x;touch /tmp/injected".into()];
        assert!(validate_options(&options).is_err());
    }

    #[test]
    fn successful_parent_does_not_leave_background_processes_holding_pipes() {
        let tool = tempfile::tempdir().unwrap();
        let options = toolchain(
            tool.path(),
            "#!/bin/sh\n(while :; do :; done) &\nprintf complete\nexit 0\n",
        );
        let isolation = tempfile::tempdir().unwrap();
        let project = isolation.path().join("project");
        fs::create_dir(&project).unwrap();
        let started = Instant::now();
        let report = execute(&project, isolation.path(), &[], &options).unwrap();
        assert!(matches!(report.status, VerificationStatus::Passed));
        assert_eq!(report.stdout, "complete");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn unsupported_stable_like_expansion_is_actionable_not_evidence() {
        let tool = tempfile::tempdir().unwrap();
        let options = toolchain(
            tool.path(),
            "#!/bin/sh\nprintf 'unstable option requires nightly' >&2\nexit 1\n",
        );
        let (root, _) = plan_fixture();
        let report = expand_machete_project(
            root.path(),
            &ExpansionOptions {
                profile: options.profiles[0].clone(),
                verification: options,
                package: None,
            },
        )
        .unwrap();
        assert!(report.expanded_rust.is_none());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|text| text.contains("already installed nightly"))
        );
        assert!(report.command.stderr.contains("requires nightly"));
    }

    #[test]
    fn explicit_linker_is_one_encoded_option_and_invalid_paths_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut options = toolchain(root.path(), "#!/bin/sh\nexit 0\n");
        let linker = root.path().join("linker with spaces");
        fs::write(&linker, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&linker, fs::Permissions::from_mode(0o755)).unwrap();
        options.linker_path = Some(linker.clone());
        validate_options(&options).unwrap();
        let command = isolated_command(root.path(), root.path(), &[], &options).unwrap();
        let flags = command
            .get_envs()
            .find(|(key, _)| *key == "CARGO_ENCODED_RUSTFLAGS")
            .unwrap()
            .1
            .unwrap();
        assert_eq!(
            flags,
            std::ffi::OsStr::new(&format!("-Clinker={}", linker.display()))
        );
        options.linker_path = Some("relative-linker".into());
        assert!(validate_options(&options).is_err());
        let separated = root.path().join("bad\u{001f}linker");
        fs::write(&separated, "").unwrap();
        options.linker_path = Some(separated);
        assert!(validate_options(&options).is_err());
    }

    #[test]
    fn expansion_report_validation_is_nonexecuting_and_checks_integrity() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='capture'\nversion='0.1.0'\n",
        )
        .unwrap();
        let tools = tempfile::tempdir().unwrap();
        let cargo = tools.path().join("cargo");
        let rustc = tools.path().join("rustc");
        // Not executable: validation must only read the selected artifacts.
        fs::write(&cargo, "cargo artifact").unwrap();
        fs::write(&rustc, "rustc artifact").unwrap();
        let mut inputs = Vec::new();
        fingerprint_copy(root.path(), root.path(), &mut inputs, &mut 0).unwrap();
        let profile = VerificationProfile {
            name: "captured".into(),
            ..Default::default()
        };
        let text = "pub fn expanded() {}\n";
        let mut report = ExpansionReport {
            profile: profile.clone(),
            cargo_path: cargo.clone(),
            rustc_path: rustc.clone(),
            cargo_identity: bounded_identity(&cargo).unwrap(),
            rustc_identity: bounded_identity(&rustc).unwrap(),
            inputs,
            command: VerificationCommandReport {
                arguments: vec![
                    "rustc".into(),
                    "--offline".into(),
                    "--locked".into(),
                    "--lib".into(),
                    "--".into(),
                    "-Zunpretty=expanded".into(),
                ],
                status: VerificationStatus::Passed,
                exit_code: Some(0),
                stdout: text.into(),
                stderr: String::new(),
                output_truncated: false,
            },
            expanded_rust: Some(text.into()),
            diagnostics: Vec::new(),
        };
        validate_expansion_report(root.path(), &report, &profile, &cargo, &rustc).unwrap();
        report.command.output_truncated = true;
        assert!(validate_expansion_report(root.path(), &report, &profile, &cargo, &rustc).is_err());
        report.command.output_truncated = false;
        report.expanded_rust = Some("pub fn forged() {}".into());
        assert!(validate_expansion_report(root.path(), &report, &profile, &cargo, &rustc).is_err());
        report.expanded_rust = Some(text.into());
        fs::write(&rustc, "changed compiler").unwrap();
        assert!(validate_expansion_report(root.path(), &report, &profile, &cargo, &rustc).is_err());
    }

    #[test]
    fn equivalent_profile_labels_and_selector_order_share_baselines() {
        let first = VerificationProfile {
            name: "first".into(),
            packages: vec!["b".into(), "a".into()],
            features: vec!["two".into(), "one".into()],
            ..Default::default()
        };
        let mut second = first.clone();
        second.name = "second".into();
        second.packages.reverse();
        second.features.reverse();
        second.features.push("one".into());
        assert_eq!(baseline_key(&first), baseline_key(&second));
        second.release = true;
        assert_ne!(baseline_key(&first), baseline_key(&second));
        second.release = false;
        second.target = Some("other-target".into());
        assert_ne!(baseline_key(&first), baseline_key(&second));
    }

    #[test]
    fn expansion_inventory_detects_added_removed_and_changed_inputs() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='capture'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(root.path().join("src/lib.rs"), "pub fn item() {}\n").unwrap();
        let mut inputs = Vec::new();
        fingerprint_copy(root.path(), root.path(), &mut inputs, &mut 0).unwrap();
        validate_capture_inputs(root.path(), &inputs).unwrap();
        // Build artifacts are deliberately outside the copied source inventory.
        fs::create_dir(root.path().join("target")).unwrap();
        fs::write(root.path().join("target/artifact"), "ignored").unwrap();
        validate_capture_inputs(root.path(), &inputs).unwrap();
        fs::write(root.path().join("src/added.rs"), "pub fn added() {}\n").unwrap();
        assert!(validate_capture_inputs(root.path(), &inputs).is_err());
        let mut added = inputs.clone();
        added.push(super::super::generated::InputFingerprint {
            path: "missing.rs".into(),
            bytes: Vec::new(),
        });
        assert!(validate_capture_inputs(root.path(), &added).is_err());
        fs::write(root.path().join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        assert!(validate_capture_inputs(root.path(), &inputs).is_err());
    }

    #[test]
    fn external_workspace_and_dependency_paths_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        for contents in [
            "[dependencies]\nx={path='../outside'}",
            "[workspace]\nmembers=['../outside']",
            "[lib]\npath='/outside/lib.rs'",
        ] {
            let manifest = root.path().join("Cargo.toml");
            fs::write(&manifest, contents).unwrap();
            assert!(validate_manifest(&manifest, root.path()).is_err());
        }
    }
}

#[cfg(not(unix))]
fn execute(
    _: &Path,
    _: &Path,
    _: &[String],
    _: &VerificationOptions,
) -> Result<VerificationCommandReport, BoxError> {
    Err("verification requires Unix process supervision".into())
}
