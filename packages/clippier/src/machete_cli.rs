//! Native dependency analysis CLI; mutation is explicit and confirmation stays here.
use std::fmt::Write as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

use clap::{Args, Subcommand, ValueEnum};
use clippier::machete::{
    DependencyKind, DependencyStatus, MacheteOptions, MacheteReport, analyze_machete,
    apply_machete_plan, plan_machete,
};
use serde::Serialize;

use super::BoxError;

#[derive(Args)]
pub struct MacheteCli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    flags: Flags,
}

#[derive(Subcommand)]
enum Command {
    /// Analyze dependencies without writing files or invoking Cargo
    Analyze,
    /// Fail when unused dependency candidates are found (read-only)
    Check,
    /// Preview exact manifest removals without modifying files
    Plan,
    /// Plan removals, then explicitly confirm before applying
    Apply,
    /// Audit suppressions and report advisory dependency-hygiene recommendations
    Audit,
    /// Explicitly execute isolated baseline and candidate Cargo verification
    #[cfg(feature = "machete-verify")]
    Verify,
    /// Explicitly request compiler-specific library macro expansion in an isolated copy
    #[cfg(feature = "machete-verify")]
    Expand,
}

#[derive(Clone, Copy, Default, ValueEnum)]
enum Output {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Normal,
    Dev,
    Build,
}

impl From<Kind> for DependencyKind {
    fn from(value: Kind) -> Self {
        match value {
            Kind::Normal => Self::Normal,
            Kind::Dev => Self::Dev,
            Kind::Build => Self::Build,
        }
    }
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)] // Independent CLI switches, not mutually exclusive state.
struct Flags {
    /// Cargo.toml path or package/workspace directory
    #[arg(long, global = true)]
    manifest_path: Option<PathBuf>,
    /// Explicit clippier.toml configuration
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Package name glob selectors (comma-separated)
    #[arg(long, global = true, value_delimiter = ',')]
    packages: Vec<String>,
    /// Dependency name/alias glob selectors (comma-separated)
    #[arg(long, global = true, value_delimiter = ',')]
    dependencies: Vec<String>,
    /// Dependency exclusions (comma-separated)
    #[arg(long, global = true, value_delimiter = ',')]
    exclude: Vec<String>,
    /// Dependency sections: normal, dev, build
    #[arg(long, global = true, value_enum, value_delimiter = ',')]
    kinds: Vec<Kind>,
    #[arg(long, global = true, value_enum, default_value = "text")]
    output: Output,
    /// Include exact manifest differences in plan/apply output
    #[arg(long, global = true)]
    diff: bool,
    /// Show every declaration and diagnostic instead of the concise summary
    #[arg(long, global = true)]
    verbose: bool,
    /// Resolve dependency library names through explicitly requested, offline locked Cargo metadata
    #[arg(long, global = true)]
    with_metadata: bool,
    /// Include files normally excluded by .gitignore/.ignore (explicit machete exclusions still apply)
    #[arg(long, global = true)]
    no_ignore: bool,
    /// Accept the displayed removal plan without prompting (apply only)
    #[arg(long, global = true)]
    yes: bool,
    /// Compatibility flag: structural scope analysis is always enabled
    #[arg(long, global = true)]
    precision: bool,
    /// Named analysis profile from clippier.toml (scoped findings are not global removal proof)
    #[arg(long = "profile", global = true)]
    analysis_profile: Option<String>,
    /// Show structured source evidence in text output (JSON always includes it)
    #[arg(long, global = true)]
    explain: bool,
    /// Explicit YYYY-MM-DD review date for suppression deadlines; never reads the clock
    #[arg(long, global = true)]
    review_date: Option<String>,
    /// Explicit JSON generated/expanded-source evidence with current-input fingerprints
    #[arg(long, global = true)]
    generated_evidence: Option<PathBuf>,
    /// Authorize execution-backed verification (never implied by --yes)
    #[cfg(feature = "machete-verify")]
    #[arg(long, global = true)]
    allow_execution: bool,
}

impl Flags {
    fn options(&self) -> MacheteOptions {
        MacheteOptions {
            manifest_path: self.manifest_path.clone(),
            config_path: self.config.clone(),
            packages: self.packages.clone(),
            dependencies: self.dependencies.clone(),
            exclude: self.exclude.clone(),
            kinds: self.kinds.iter().copied().map(Into::into).collect(),
            no_ignore: self.no_ignore.then_some(true),
            precision: true,
            review_date: self.review_date.clone(),
            profile: self.analysis_profile.clone(),
            ..MacheteOptions::default()
        }
    }
}

#[derive(Serialize)]
struct Envelope<'a> {
    schema_version: u32,
    outcome: &'a str,
    report: Option<&'a MacheteReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changes: Option<&'a [clippier::machete::MacheteChange]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
}

fn emit(flags: &Flags, result: &Envelope<'_>) -> Result<(), BoxError> {
    let mut out = io::stdout().lock();
    match flags.output {
        Output::Json => {
            serde_json::to_writer(&mut out, result)?;
            writeln!(out)?;
        }
        Output::Text => {
            if let Some(report) = result.report {
                render_analysis(&mut out, flags, result.outcome, report)?;
            }
            if let Some(diff) = result.diff {
                write!(out, "{diff}")?;
            }
            if let Some(error) = result.error {
                writeln!(out, "Error: {error}")?;
            }
        }
    }
    Ok(())
}

fn render_analysis(
    out: &mut impl Write,
    flags: &Flags,
    outcome: &str,
    report: &MacheteReport,
) -> Result<(), BoxError> {
    let count = |status| {
        report
            .dependencies
            .iter()
            .filter(|d| d.status == status)
            .count()
    };
    let packages = report
        .dependencies
        .iter()
        .map(|d| &d.manifest_path)
        .collect::<BTreeSet<_>>()
        .len();
    let unused = count(DependencyStatus::Unused);
    let uncertain = count(DependencyStatus::Incomplete);
    writeln!(
        out,
        "Machete | {}",
        if report.applied { "APPLIED" } else { outcome }
    )?;
    writeln!(
        out,
        "{packages} packages, {} dependency declarations",
        report.dependencies.len()
    )?;
    writeln!(
        out,
        "{unused} unused, {uncertain} unverified, {} used, {} ignored, {} not selected",
        count(DependencyStatus::Used),
        count(DependencyStatus::Ignored),
        count(DependencyStatus::Skipped)
    )?;
    writeln!(out)?;
    if unused == 0 {
        writeln!(
            out,
            "No unused dependencies identified{}",
            if report.complete {
                "."
            } else {
                " (analysis incomplete)."
            }
        )?;
    }
    render_findings(out, report)?;
    render_diagnostics(out, report)?;
    if !report.complete {
        writeln!(
            out,
            "\nAnalysis is incomplete: this is not proof that the workspace has no unused dependencies."
        )?;
        writeln!(
            out,
            "Apply is blocked for incomplete analysis; check exits with status 2."
        )?;
    }
    if report.applied {
        writeln!(
            out,
            "\nManifest changes applied; Cargo.lock preserved: {}.",
            report.lockfile_preserved
        )?;
    }
    if !report.diagnostics.is_empty() || !report.complete {
        writeln!(
            out,
            "\nDetails: --verbose for full diagnostics; --explain for source evidence; --output json for structured results."
        )?;
        writeln!(out, "Use --packages <name> to focus on a package.")?;
    }
    if flags.verbose {
        writeln!(out, "\nAll declarations:")?;
        for dep in &report.dependencies {
            writeln!(
                out,
                "  {} / {} ({:?}): {:?}: {}",
                dep.package, dep.dependency, dep.kind, dep.status, dep.reason
            )?;
        }
        writeln!(out, "\nFull diagnostics:")?;
        for diagnostic in &report.diagnostics {
            writeln!(
                out,
                "  {}",
                diagnostic.replace(&format!("{}/", report.root.display()), "")
            )?;
        }
    }
    if flags.explain {
        writeln!(
            out,
            "\nCoverage: {}",
            serde_json::to_string(&report.coverage)?
        )?;
        for evidence in &report.evidence {
            writeln!(out, "Evidence: {}", serde_json::to_string(evidence)?)?;
        }
    }
    Ok(())
}

fn render_findings(out: &mut impl Write, report: &MacheteReport) -> Result<(), BoxError> {
    for (status, heading) in [
        (DependencyStatus::Unused, "Unused dependencies"),
        (
            DependencyStatus::Incomplete,
            "Unverified dependencies (not removable)",
        ),
    ] {
        if !report.dependencies.iter().any(|dep| dep.status == status) {
            continue;
        }
        writeln!(out, "{heading}:")?;
        let mut previous = None;
        for dep in report
            .dependencies
            .iter()
            .filter(|dep| dep.status == status)
        {
            if previous != Some(&dep.manifest_path) {
                let relative = dep
                    .manifest_path
                    .strip_prefix(&report.root)
                    .unwrap_or(&dep.manifest_path);
                writeln!(out, "  {} ({})", dep.package, relative.display())?;
                previous = Some(&dep.manifest_path);
            }
            let kind = match dep.kind {
                DependencyKind::Normal => "normal",
                DependencyKind::Build => "build",
                DependencyKind::Dev => "dev",
            };
            write!(out, "    {} [{kind}]", dep.dependency)?;
            if let Some(target) = &dep.target {
                write!(out, " [{target}]")?;
            }
            writeln!(out, " - {}", dep.reason)?;
        }
    }
    Ok(())
}

// Presentation-only grouping; JSON retains every original diagnostic unchanged.
fn render_diagnostics(out: &mut impl Write, report: &MacheteReport) -> Result<(), BoxError> {
    if report.diagnostics.is_empty() {
        return Ok(());
    }
    let mut groups = BTreeMap::<(bool, String), usize>::new();
    let advisory_count = report
        .diagnostics
        .iter()
        .filter(|d| d.starts_with("advisory:"))
        .count();
    for diagnostic in &report.diagnostics {
        let advisory = diagnostic.starts_with("advisory:");
        let text = diagnostic.strip_prefix("advisory: ").unwrap_or(diagnostic);
        let reason = text.split_once(": ").map_or(text, |(_, reason)| reason);
        let category = if reason.contains("unexpanded Rust macro") {
            "Unexpanded macros/attributes; generated references are unknown"
        } else if reason.starts_with("imported alias ") {
            "Import aliases with unresolved type/value namespaces"
        } else {
            reason
        };
        *groups.entry((advisory, category.to_owned())).or_default() += 1;
    }
    let mut groups = groups.into_iter().collect::<Vec<_>>();
    // Inventory failures precede advisories; frequent causes precede rare ones.
    groups.sort_by(
        |((a_advisory, a_reason), a_count), ((b_advisory, b_reason), b_count)| {
            a_advisory
                .cmp(b_advisory)
                .then_with(|| b_count.cmp(a_count))
                .then_with(|| a_reason.cmp(b_reason))
        },
    );
    writeln!(
        out,
        "\nCoverage diagnostics: {} ({} limitations, {advisory_count} advisories)",
        report.diagnostics.len(),
        report.diagnostics.len() - advisory_count
    )?;
    for ((advisory, reason), count) in groups.iter().take(5) {
        writeln!(
            out,
            "  {count:>5}  {}{reason}",
            if *advisory { "[advisory] " } else { "" }
        )?;
    }
    if groups.len() > 5 {
        writeln!(
            out,
            "  ... {} more diagnostic categories; see --verbose",
            groups.len() - 5
        )?;
    }
    Ok(())
}

fn exact_diff(changes: &[clippier::machete::MacheteChange]) -> String {
    let mut result = String::new();
    for change in changes {
        writeln!(
            result,
            "--- {}\n+++ {}",
            change.path.display(),
            change.path.display()
        )
        .expect("writing to a String cannot fail");
        // Whole-file unified hunks are intentionally exact, including unchanged lines.
        writeln!(
            result,
            "@@ -1,{} +1,{} @@",
            change.before.lines().count(),
            change.after.lines().count()
        )
        .expect("writing to a String cannot fail");
        for line in change.before.lines() {
            result.push('-');
            result.push_str(line);
            result.push('\n');
        }
        if !change.before.is_empty() && !change.before.ends_with('\n') {
            result.push_str("\\ No newline at end of file\n");
        }
        for line in change.after.lines() {
            result.push('+');
            result.push_str(line);
            result.push('\n');
        }
        if !change.after.is_empty() && !change.after.ends_with('\n') {
            result.push_str("\\ No newline at end of file\n");
        }
    }
    result
}

fn has_unused(report: &MacheteReport) -> bool {
    report
        .dependencies
        .iter()
        .any(|dependency| dependency.status == DependencyStatus::Unused)
}

fn confirm(flags: &Flags, plan: &clippier::machete::MachetePlan) -> Result<bool, BoxError> {
    if flags.yes {
        return Ok(true);
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Ok(false);
    }
    eprintln!(
        "Proposed manifest removals (Cargo.lock is preserved):\n{}",
        exact_diff(&plan.changes)
    );
    eprint!("Apply dependency removals? [y/N] ");
    io::stderr().flush()?;
    let mut response = String::new();
    io::stdin().read_line(&mut response)?;
    Ok(matches!(
        response.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn validate_flags(cli: &MacheteCli) -> Result<(bool, bool), BoxError> {
    let applying = matches!(cli.command, Some(Command::Apply));
    let planning = applying || matches!(cli.command, Some(Command::Plan));
    if cli.flags.yes && !applying {
        return Err("--yes requires machete apply".into());
    }
    if cli.flags.diff && !planning {
        return Err("--diff requires machete plan or apply".into());
    }
    #[cfg(feature = "machete-verify")]
    if cli.flags.allow_execution && (planning || matches!(cli.command, Some(Command::Audit))) {
        return Err("--allow-execution is supported for analyze/check/verify/expand; incomplete plans cannot be applied, even after finite compiler checks".into());
    }
    Ok((applying, planning))
}

fn emit_extra<T: Serialize>(flags: &Flags, field: &str, report: &T) -> Result<(), BoxError> {
    let value = serde_json::json!({"schema_version": 1, "outcome": "ANALYZED", field: report});
    let stdout = io::stdout();
    let mut output = stdout.lock();
    match flags.output {
        Output::Json => serde_json::to_writer_pretty(&mut output, &value)?,
        Output::Text => writeln!(output, "{}", serde_json::to_string_pretty(&value)?)?,
    }
    writeln!(output)?;
    Ok(())
}

fn report_outcome(report: &MacheteReport, planning: bool) -> &'static str {
    if !report.complete {
        "INCOMPLETE"
    } else if planning {
        "PLANNED"
    } else if has_unused(report) {
        "FINDINGS"
    } else {
        "CLEAN"
    }
}

fn collect_metadata(options: &mut MacheteOptions) -> Result<(), BoxError> {
    let path = options
        .manifest_path
        .clone()
        .unwrap_or(std::env::current_dir()?);
    let manifest = if path.is_dir() {
        path.join("Cargo.toml")
    } else {
        path
    };
    let manifest = std::fs::canonicalize(manifest)?;
    // Explicitly opted in: locked/offline metadata cannot update project manifests or lockfile.
    let output = std::process::Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()?;
    if !output.status.success() {
        return Err(format!("explicit Cargo metadata failed (requires an existing up-to-date lockfile and cached dependencies): {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    options.metadata = Some(clippier::machete::MacheteMetadata::from_cargo_json(
        std::str::from_utf8(&output.stdout)?,
    )?);
    Ok(())
}

fn run_special(cli: &MacheteCli, options: &MacheteOptions) -> Result<Option<u8>, BoxError> {
    if matches!(cli.command, Some(Command::Audit)) {
        emit_extra(
            &cli.flags,
            "audit",
            &clippier::machete::audit_machete(options)?,
        )?;
        return Ok(Some(0));
    }
    #[cfg(feature = "machete-verify")]
    if matches!(cli.command, Some(Command::Verify | Command::Expand)) {
        return run_execution(cli, options).map(Some);
    }
    Ok(None)
}

#[cfg(feature = "machete-verify")]
fn run_execution(cli: &MacheteCli, options: &MacheteOptions) -> Result<u8, BoxError> {
    use clippier::machete::verify::{
        ExpansionOptions, VerificationStatus, expand_machete_project, verify_machete,
    };
    if !cli.flags.allow_execution {
        return Err("verification/expansion requires --allow-execution; build scripts and procedural macros may execute".into());
    }
    let mut policy = clippier::machete::load_verification_options(options)?
        .ok_or("execution requires [machete.verification] in clippier.toml")?;
    policy.allow_execution = true;
    if matches!(cli.command, Some(Command::Expand)) {
        let profile = policy
            .profiles
            .first()
            .cloned()
            .ok_or("expansion requires one configured verification profile")?;
        if policy.profiles.len() != 1 || options.packages.len() > 1 {
            return Err(
                "expansion requires one profile and at most one exact package selector".into(),
            );
        }
        let report = analyze_machete(options)?;
        let expansion = expand_machete_project(
            &report.root,
            &ExpansionOptions {
                verification: policy,
                profile,
                package: options.packages.first().cloned(),
            },
        )?;
        let code = u8::from(expansion.expanded_rust.is_none());
        emit_extra(&cli.flags, "expansion", &expansion)?;
        return Ok(code);
    }
    let report = verify_machete(options, &policy)?;
    let code = u8::from(
        report.profiles.is_empty()
            || !report.profiles.iter().all(|profile| {
                matches!(profile.baseline.status, VerificationStatus::Passed)
                    && matches!(profile.candidate_result.status, VerificationStatus::Passed)
            }),
    );
    emit_extra(&cli.flags, "verification", &report)?;
    Ok(code)
}

type AnalysisResult = Result<(MacheteReport, Option<clippier::machete::MachetePlan>), u8>;

fn prepare(cli: &MacheteCli, planning: bool) -> Result<AnalysisResult, BoxError> {
    let flags = &cli.flags;
    let mut options = flags.options();
    (|| -> Result<_, BoxError> {
        if let Some(path) = &flags.generated_evidence {
            options.generated = Some(serde_json::from_slice(&std::fs::read(path)?)?);
        }
        if flags.with_metadata {
            collect_metadata(&mut options)?;
        }
        if let Some(code) = run_special(cli, &options)? {
            return Ok(Err(code));
        }
        #[cfg(feature = "machete-verify")]
        if flags.allow_execution {
            // Compiler observations supplement, never promote, global source findings.
            let mut policy = clippier::machete::load_verification_options(&options)?
                .ok_or("--allow-execution requires [machete.verification] in clippier.toml")?;
            policy.allow_execution = true;
            let analysis = analyze_machete(&options)?;
            let verification = clippier::machete::verify::verify_machete(&options, &policy)?;
            emit_extra(
                flags,
                "analysis",
                &serde_json::json!({
                    "report": analysis, "verification": verification,
                    "publication_authorized": false,
                    "note": "Finite compiler results do not resolve global uncertainty; apply remains blocked for incomplete reports."
                }),
            )?;
            let compiler_complete = verification.profiles.iter().all(|profile| {
                matches!(
                    profile.outcome,
                    clippier::machete::verify::VerificationOutcome::CandidateCompiled
                )
            });
            let code = if analysis.complete && compiler_complete {
                u8::from(matches!(cli.command, Some(Command::Check)) && has_unused(&analysis))
            } else {
                2
            };
            return Ok(Err(code));
        }
        if planning {
            plan_machete(&options).map(|plan| Ok((plan.report.clone(), Some(plan))))
        } else {
            analyze_machete(&options).map(|report| Ok((report, None)))
        }
    })()
}

pub fn run(cli: &MacheteCli) -> Result<u8, BoxError> {
    let flags = &cli.flags;
    let (applying, planning) = validate_flags(cli)?;
    let result = prepare(cli, planning);
    let (report, plan) = match result {
        Ok(Ok(value)) => value,
        Ok(Err(code)) => return Ok(code),
        Err(error) => {
            emit(
                flags,
                &Envelope {
                    schema_version: 1,
                    outcome: "FAILED",
                    report: None,
                    changes: None,
                    diff: None,
                    error: Some(&error.to_string()),
                },
            )?;
            return Ok(2);
        }
    };
    let diff = plan
        .as_ref()
        .filter(|_| flags.diff)
        .map(|plan| exact_diff(&plan.changes));
    if applying && report.complete && plan.as_ref().is_some_and(|p| !p.changes.is_empty()) {
        let plan = plan.as_ref().expect("apply always constructs a plan");
        let accepted = confirm(flags, plan)?;
        if !accepted {
            emit(
                flags,
                &Envelope {
                    schema_version: 1,
                    outcome: "CANCELLED",
                    report: Some(&report),
                    changes: Some(&plan.changes),
                    diff: diff.as_deref(),
                    error: None,
                },
            )?;
            return Ok(1);
        }
        match apply_machete_plan(plan) {
            Ok(applied) => {
                emit(
                    flags,
                    &Envelope {
                        schema_version: 1,
                        outcome: "APPLIED",
                        report: Some(&applied),
                        changes: Some(&plan.changes),
                        diff: diff.as_deref(),
                        error: None,
                    },
                )?;
                return Ok(0);
            }
            Err(error) => {
                emit(
                    flags,
                    &Envelope {
                        schema_version: 1,
                        outcome: "FAILED",
                        report: Some(&report),
                        changes: Some(&plan.changes),
                        diff: diff.as_deref(),
                        error: Some(&error.to_string()),
                    },
                )?;
                return Ok(2);
            }
        }
    }
    let outcome = report_outcome(&report, planning);
    emit(
        flags,
        &Envelope {
            schema_version: 1,
            outcome,
            report: Some(&report),
            changes: plan.as_ref().map(|plan| plan.changes.as_slice()),
            diff: diff.as_deref(),
            error: None,
        },
    )?;
    Ok(if report.complete {
        u8::from(matches!(cli.command, Some(Command::Check)) && has_unused(&report))
    } else {
        2
    })
}
