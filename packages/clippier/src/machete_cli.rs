//! Native dependency analysis CLI; mutation is explicit and confirmation stays here.
use std::fmt::Write as _;
use std::{
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
    /// Resolve dependency library names through explicitly requested, offline locked Cargo metadata
    #[arg(long, global = true)]
    with_metadata: bool,
    /// Include files normally excluded by .gitignore/.ignore (explicit machete exclusions still apply)
    #[arg(long, global = true)]
    no_ignore: bool,
    /// Accept the displayed removal plan without prompting (apply only)
    #[arg(long, global = true)]
    yes: bool,
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
                writeln!(out, "Workspace: {}", report.root.display())?;
                for dep in &report.dependencies {
                    if dep.status != DependencyStatus::Used {
                        writeln!(
                            out,
                            "{} / {} ({:?}): {:?}: {}",
                            dep.package, dep.dependency, dep.kind, dep.status, dep.reason
                        )?;
                    }
                }
                for diagnostic in &report.diagnostics {
                    writeln!(out, "Diagnostic: {diagnostic}")?;
                }
                writeln!(
                    out,
                    "Outcome: {}; complete: {}; applied: {}; Cargo.lock preserved: {}",
                    result.outcome, report.complete, report.applied, report.lockfile_preserved
                )?;
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
    Ok((applying, planning))
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

pub fn run(cli: &MacheteCli) -> Result<u8, BoxError> {
    let flags = &cli.flags;
    let (applying, planning) = validate_flags(cli)?;
    let mut options = flags.options();
    let result = (|| -> Result<_, BoxError> {
        if flags.with_metadata {
            collect_metadata(&mut options)?;
        }
        if planning {
            plan_machete(&options).map(|plan| (plan.report.clone(), Some(plan)))
        } else {
            analyze_machete(&options).map(|report| (report, None))
        }
    })();
    let (report, plan) = match result {
        Ok(value) => value,
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
