//! Clippier - Rust workspace analysis and automation CLI tool.
//!
//! This binary provides a command-line interface for workspace analysis, CI/CD generation,
//! dependency management, and feature validation in Rust workspace projects.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[cfg(any(feature = "check", feature = "format", feature = "publish"))]
use clippier::ColorMode;
#[cfg(feature = "publish")]
use clippier::preferences::{PreferencesOverrides, PurityPolicy, resolve_preferences};
#[cfg(feature = "release")]
use clippier::release::{
    IndependentizeConfig, ReleasePrepareConfig, ReleasePublishConfig, ReleaseStatusConfig,
    handle_independentize_command, handle_release_prepare_command, handle_release_publish_command,
    handle_release_status_command, handle_release_verify_command,
};
#[cfg(feature = "versioning")]
use clippier::versioning::{VersionBump, VersionConfig, VersionOperation, handle_version_command};
use clippier::{
    OutputType, handle_affected_packages_command, handle_ci_steps_command,
    handle_dependencies_command, handle_environment_command, handle_features_command,
    handle_generate_dockerfile_command, handle_packages_command,
    handle_validate_feature_propagation_command, handle_workspace_deps_command,
    handle_workspace_toolchains_command, print_human_output,
};
#[cfg(feature = "publish")]
use clippier::{PublishConfig, handle_publish_command};

#[cfg(feature = "check")]
use clippier::handle_check_command;
#[cfg(feature = "format")]
use clippier::handle_fmt_command;
#[cfg(any(feature = "check", feature = "format"))]
use clippier::tools::build_tools_config;
#[cfg(feature = "format")]
use clippier::tools::{FormatScope, apply_format_cli_overrides};

#[cfg(feature = "machete")]
mod machete_cli;

#[cfg(feature = "upgrade")]
mod upgrade_cli {
    use std::{
        io::{self, BufRead, IsTerminal, Write},
        path::PathBuf,
    };

    use clap::{Args, Subcommand, ValueEnum};
    use clippier::upgrade::{
        DependencyKind, LockfilePolicy, PinnedPolicy, PrereleasePolicy, RegistryOptions,
        RegistryOverrides, SourcePolicy, UpgradeOptions, UpgradePolicy, UpgradeReport,
        UpgradeStatus, apply_upgrade_plan, build_upgrade_plan,
    };
    use serde::Serialize;

    use super::BoxError;

    #[derive(Args)]
    pub struct UpgradeCli {
        #[command(subcommand)]
        command: Option<UpgradeCommand>,
        #[command(flatten)]
        flags: UpgradeFlags,
    }

    #[derive(Subcommand)]
    enum UpgradeCommand {
        /// Preview upgrades without modifying project files
        Plan,
    }

    #[derive(Clone, Copy, Default, ValueEnum)]
    enum Output {
        #[default]
        Text,
        Json,
    }

    #[derive(Args)]
    #[allow(clippy::struct_excessive_bools)] // Independent, presence-tracked CLI switches.
    struct UpgradeFlags {
        /// Cargo.toml or workspace directory (defaults to the current directory)
        #[arg(long, global = true)]
        manifest_path: Option<PathBuf>,
        /// Explicit clippier.toml (otherwise discovered at the workspace root)
        #[arg(long = "config", alias = "config-path", global = true)]
        config_path: Option<PathBuf>,
        /// Dependency names or aliases to include (glob patterns)
        #[arg(
            short = 'd',
            long = "dependency",
            alias = "dependencies",
            value_delimiter = ',',
            global = true
        )]
        dependencies: Vec<String>,
        /// Workspace packages to include (glob patterns)
        #[arg(
            short = 'p',
            long = "package",
            alias = "packages",
            value_delimiter = ',',
            global = true
        )]
        packages: Vec<String>,
        /// Dependency names or aliases to exclude (glob patterns)
        #[arg(long, value_delimiter = ',', global = true)]
        exclude: Vec<String>,
        /// Dependency kinds: normal, dev, build
        #[arg(long, alias = "kind", value_enum, value_delimiter = ',', global = true)]
        kinds: Vec<DependencyKind>,
        /// Stay within existing compatibility requirements (the default)
        #[arg(long, conflicts_with = "incompatible", global = true)]
        compatible: bool,
        /// Allow requirements to cross compatibility boundaries
        #[arg(long, conflicts_with = "lockfile_only", global = true)]
        incompatible: bool,
        /// Preserve or upgrade exact version pins
        #[arg(long, alias = "pins", value_enum, global = true)]
        pinned: Option<PinnedPolicy>,
        /// Preserve prerelease eligibility or allow prereleases
        #[arg(long, value_enum, global = true)]
        prerelease: Option<PrereleasePolicy>,
        /// Explicit safe default: Git dependencies are always excluded from upgrades
        #[arg(long, global = true)]
        skip_git_dependencies: bool,
        /// Registry-only upgrades, or include versioned path dependencies
        #[arg(long, value_enum, global = true)]
        source: Option<SourcePolicy>,
        /// Explicit Rust compiler version ceiling for candidate releases
        #[arg(long, global = true)]
        msrv: Option<String>,
        /// Lockfile policy (overrides configuration)
        #[arg(long, value_enum, global = true)]
        lockfile: Option<LockfilePolicy>,
        /// Disable lockfile updates, overriding configuration
        #[arg(long, conflicts_with_all = ["lockfile", "lockfile_only", "resolve_lockfile"], global = true)]
        no_lockfile: bool,
        /// Update only Cargo.lock, without changing manifest requirements
        #[arg(long, global = true)]
        lockfile_only: bool,
        /// Resolve an exact lockfile preview in an isolated snapshot
        #[arg(long, global = true)]
        resolve_lockfile: bool,
        /// Preview only; equivalent to `upgrade plan`
        #[arg(long, global = true)]
        dry_run: bool,
        /// Skip confirmation only; all planning and safety checks still run
        #[arg(short = 'y', long, global = true)]
        yes: bool,
        /// Include the planned diff (unresolved lockfiles are labeled)
        #[arg(long, global = true)]
        diff: bool,
        /// Report format; JSON emits exactly one report on stdout
        #[arg(long, value_enum, default_value = "text", global = true)]
        output: Output,
        /// Preview only, exiting unsuccessfully if changes are needed
        #[arg(long, global = true)]
        check: bool,
        /// Use only locally cached registry data
        #[arg(long, global = true)]
        offline: bool,
        /// Registry request timeout in seconds
        #[arg(long, alias = "registry-timeout-seconds", value_parser = clap::value_parser!(u64).range(1..), global = true)]
        timeout_seconds: Option<u64>,
        /// Maximum registry request retries
        #[arg(long, alias = "registry-max-retries", global = true)]
        max_retries: Option<u32>,
        /// Maximum simultaneous registry requests
        #[arg(long, alias = "registry-max-concurrency", value_parser = positive_usize, global = true)]
        max_concurrency: Option<usize>,
    }

    fn positive_usize(value: &str) -> Result<usize, String> {
        value
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| "expected a positive integer".to_owned())
    }

    impl UpgradeFlags {
        fn options(&self) -> UpgradeOptions {
            let defaults = RegistryOptions::default();
            UpgradeOptions {
                manifest_path: self.manifest_path.clone(),
                config_path: self.config_path.clone(),
                dependencies: self.dependencies.clone(),
                packages: self.packages.clone(),
                exclude: self.exclude.clone(),
                kinds: self.kinds.clone(),
                policy: if self.incompatible {
                    Some(UpgradePolicy::Incompatible)
                } else if self.compatible {
                    Some(UpgradePolicy::Compatible)
                } else {
                    None
                },
                pinned: self.pinned,
                prerelease: self.prerelease,
                source: self.source,
                lockfile: if self.no_lockfile {
                    Some(LockfilePolicy::None)
                } else {
                    self.lockfile
                },
                msrv: self.msrv.clone(),
                lockfile_only: self.lockfile_only,
                resolve_lockfile: self.resolve_lockfile,
                registry_overrides: RegistryOverrides {
                    offline: self.offline.then_some(true),
                    timeout_seconds: self.timeout_seconds,
                    max_retries: self.max_retries,
                    max_concurrency: self.max_concurrency,
                },
                registry: RegistryOptions {
                    offline: self.offline,
                    timeout_seconds: self.timeout_seconds.unwrap_or(defaults.timeout_seconds),
                    max_retries: self.max_retries.unwrap_or(defaults.max_retries),
                    max_concurrency: self.max_concurrency.unwrap_or(defaults.max_concurrency),
                },
            }
        }
    }

    #[derive(Clone, Copy, Debug, Serialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    enum Outcome {
        Planned,
        Unchanged,
        Applied,
        CheckFailed,
        Cancelled,
        Blocked,
        PlanningFailed,
        ApplyFailed,
    }

    #[derive(Clone, Copy, Serialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    enum Scope {
        Plan,
        Check,
        Apply,
    }

    // These are CLI overrides, not a reconstruction of the engine's merged config.
    #[derive(Serialize)]
    struct RequestedSettings<'a> {
        policy: Option<UpgradePolicy>,
        pinned: Option<PinnedPolicy>,
        prerelease: Option<PrereleasePolicy>,
        source: Option<SourcePolicy>,
        lockfile_policy: Option<LockfilePolicy>,
        msrv: Option<&'a str>,
        lockfile_only: bool,
        resolve_lockfile: bool,
        offline: Option<bool>,
        timeout_seconds: Option<u64>,
        max_retries: Option<u32>,
        max_concurrency: Option<usize>,
        packages: &'a [String],
        dependencies: &'a [String],
        exclude: &'a [String],
        kinds: &'a [DependencyKind],
    }

    #[derive(Serialize)]
    struct Envelope<'a, 'r> {
        schema_version: u32,
        scope: Scope,
        cli_overrides: RequestedSettings<'a>,
        #[serde(flatten)]
        result: &'a Report<'r>,
    }

    #[derive(Serialize)]
    struct Report<'a> {
        outcome: Outcome,
        #[serde(rename = "report")]
        details: Option<&'a UpgradeReport>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diff: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<&'a str>,
    }

    fn write_text(writer: &mut impl Write, report: &UpgradeReport) -> io::Result<()> {
        writeln!(writer, "Workspace: {}", report.workspace_root.display())?;
        for dependency in &report.dependencies {
            writeln!(
                writer,
                "{} / {}: {} -> {} ({:?}: {})",
                dependency.package,
                dependency.alias,
                dependency.previous.as_deref().unwrap_or("-"),
                dependency.requirement.as_deref().unwrap_or("-"),
                dependency.status,
                dependency.reason
            )?;
        }
        writeln!(
            writer,
            "Manifests: {}; lockfile: {:?}; applied: {}",
            report.manifests.len(),
            report.lockfile,
            report.applied
        )
    }

    fn emit(flags: &UpgradeFlags, scope: Scope, report: &Report<'_>) -> Result<(), BoxError> {
        let mut stdout = io::stdout().lock();
        match flags.output {
            Output::Json => {
                let options = flags.options();
                let envelope = Envelope {
                    schema_version: 1,
                    scope,
                    cli_overrides: RequestedSettings {
                        policy: options.policy,
                        pinned: options.pinned,
                        prerelease: options.prerelease,
                        source: options.source,
                        lockfile_policy: options.lockfile,
                        msrv: flags.msrv.as_deref(),
                        lockfile_only: flags.lockfile_only,
                        resolve_lockfile: flags.resolve_lockfile,
                        offline: options.registry_overrides.offline,
                        timeout_seconds: flags.timeout_seconds,
                        max_retries: flags.max_retries,
                        max_concurrency: flags.max_concurrency,
                        packages: &flags.packages,
                        dependencies: &flags.dependencies,
                        exclude: &flags.exclude,
                        kinds: &flags.kinds,
                    },
                    result: report,
                };
                serde_json::to_writer(&mut stdout, &envelope)?;
                writeln!(stdout)?;
            }
            Output::Text => {
                writeln!(stdout, "{:?}", report.outcome)?;
                if let Some(inner) = report.details {
                    write_text(&mut stdout, inner)?;
                }
                if let Some(diff) = report.diff {
                    write!(stdout, "{diff}")?;
                }
            }
        }
        if let Some(error) = report.error {
            eprintln!("{error}");
        }
        Ok(())
    }

    // One read only: EOF, empty input, and every answer other than y/yes cancel.
    fn read_confirmation(reader: &mut impl BufRead) -> io::Result<bool> {
        let mut answer = String::new();
        reader.read_line(&mut answer)?;
        Ok(matches!(
            answer.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ))
    }

    const fn confirmation_needed(yes: bool, has_work: bool) -> bool {
        has_work && !yes
    }

    #[allow(clippy::too_many_lines)]
    pub async fn run(command: &UpgradeCli) -> Result<u8, BoxError> {
        let flags = &command.flags;
        let scope = if flags.check {
            Scope::Check
        } else if command.command.is_some() || flags.dry_run {
            Scope::Plan
        } else {
            Scope::Apply
        };
        let plan = match build_upgrade_plan(&flags.options()).await {
            Ok(plan) => plan,
            Err(error) => {
                emit(
                    flags,
                    scope,
                    &Report {
                        outcome: Outcome::PlanningFailed,
                        details: None,
                        diff: None,
                        error: Some(&error.to_string()),
                    },
                )?;
                return Ok(1);
            }
        };
        let diff = plan.diff();
        let report = plan.report();
        let has_work = report.pending;
        let shown_diff = flags.diff.then_some(diff.as_str());
        if !report.blockers.is_empty() {
            let message = report.blockers.join("; ");
            emit(
                flags,
                scope,
                &Report {
                    outcome: Outcome::Blocked,
                    details: Some(report),
                    diff: shown_diff,
                    error: Some(&message),
                },
            )?;
            return Ok(1);
        }
        let preview = command.command.is_some() || flags.dry_run || flags.check;
        if preview || !has_work {
            let failed_check = flags.check && has_work;
            emit(
                flags,
                scope,
                &Report {
                    outcome: if failed_check {
                        Outcome::CheckFailed
                    } else if has_work {
                        Outcome::Planned
                    } else {
                        Outcome::Unchanged
                    },
                    details: Some(report),
                    diff: shown_diff,
                    error: None,
                },
            )?;
            return Ok(u8::from(failed_check));
        }
        if confirmation_needed(flags.yes, has_work) {
            // JSON stdout must remain a single final report. Preview and prompt use stderr.
            let mut stderr = io::stderr().lock();
            write_text(&mut stderr, report)?;
            if let Some(diff) = shown_diff {
                write!(stderr, "{diff}")?;
            }
            if !io::stdin().is_terminal()
                || !io::stdout().is_terminal()
                || !io::stderr().is_terminal()
            {
                drop(stderr);
                emit(
                    flags,
                    scope,
                    &Report {
                        outcome: Outcome::Blocked,
                        details: Some(report),
                        diff: shown_diff,
                        error: Some(
                            "Confirmation requires terminal input/output; use --yes to apply or --dry-run to preview.",
                        ),
                    },
                )?;
                return Ok(1);
            }
            write!(stderr, "Apply these upgrades? [y/N] ")?;
            stderr.flush()?;
            drop(stderr);
            let confirmation = read_confirmation(&mut io::stdin().lock());
            match confirmation {
                Ok(true) => {}
                answer => {
                    let message = match answer {
                        Err(error) => {
                            format!("Upgrade cancelled: could not read confirmation: {error}")
                        }
                        _ => "Upgrade cancelled; no changes applied.".to_owned(),
                    };
                    let mut cancelled = report.clone();
                    cancelled.cancelled = true;
                    cancelled.status = UpgradeStatus::Cancelled;
                    emit(
                        flags,
                        scope,
                        &Report {
                            outcome: Outcome::Cancelled,
                            details: Some(&cancelled),
                            diff: shown_diff,
                            error: Some(&message),
                        },
                    )?;
                    return Ok(1);
                }
            }
        }
        match apply_upgrade_plan(&plan).await {
            Ok(applied) => {
                emit(
                    flags,
                    scope,
                    &Report {
                        outcome: Outcome::Applied,
                        details: Some(&applied),
                        diff: shown_diff,
                        error: None,
                    },
                )?;
                Ok(0)
            }
            Err(error) => {
                let mut failed = report.clone();
                failed.status = UpgradeStatus::Failed;
                emit(
                    flags,
                    scope,
                    &Report {
                        outcome: Outcome::ApplyFailed,
                        details: Some(&failed),
                        diff: shown_diff,
                        error: Some(&error.to_string()),
                    },
                )?;
                Ok(1)
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::io::Cursor;

        use clap::Parser;

        use super::*;

        fn parse(args: &[&str]) -> UpgradeCli {
            let args = crate::Args::try_parse_from(args).unwrap();
            let crate::Commands::Upgrade(command) = args.cmd else {
                panic!("expected upgrade")
            };
            command
        }

        #[test]
        fn default_applies_but_yes_only_changes_confirmation() {
            let default = parse(&["clippier", "upgrade"]);
            let yes = parse(&[
                "clippier",
                "upgrade",
                "--skip-git-dependencies",
                "--compatible",
                "-y",
            ]);
            assert!(default.command.is_none());
            assert!(!default.flags.dry_run);
            assert!(default.flags.options().policy.is_none());
            assert!(yes.flags.yes);
            assert!(yes.flags.skip_git_dependencies);
            assert_eq!(yes.flags.options().policy, Some(UpgradePolicy::Compatible));
            assert_eq!(
                default.flags.options().lockfile,
                yes.flags.options().lockfile
            );
            assert!(default.flags.options().registry_overrides.offline.is_none());
            assert!(
                default
                    .flags
                    .options()
                    .registry_overrides
                    .timeout_seconds
                    .is_none()
            );
            assert!(
                default
                    .flags
                    .options()
                    .registry_overrides
                    .max_retries
                    .is_none()
            );
            assert!(
                default
                    .flags
                    .options()
                    .registry_overrides
                    .max_concurrency
                    .is_none()
            );
            assert!(confirmation_needed(false, true));
            assert!(!confirmation_needed(true, true));
            assert!(!confirmation_needed(false, false));
        }

        #[test]
        fn plan_accepts_options_before_and_after_subcommand() {
            let command = parse(&[
                "clippier",
                "upgrade",
                "--offline",
                "plan",
                "--compatible",
                "--dependency",
                "serde,tokio",
                "-p",
                "app",
                "--exclude",
                "old*",
                "--kinds",
                "normal,dev,build",
                "--no-lockfile",
                "--output",
                "json",
            ]);
            assert!(command.command.is_some());
            let options = command.flags.options();
            assert!(options.registry.offline);
            assert_eq!(options.policy, Some(UpgradePolicy::Compatible));
            assert_eq!(options.lockfile, Some(LockfilePolicy::None));
            assert_eq!(options.dependencies, ["serde", "tokio"]);
            assert_eq!(options.packages, ["app"]);
            assert_eq!(options.exclude, ["old*"]);
            assert_eq!(
                options.kinds,
                [
                    DependencyKind::Normal,
                    DependencyKind::Dev,
                    DependencyKind::Build
                ]
            );
        }

        #[test]
        fn rejects_conflicts_and_invalid_limits() {
            for args in [
                vec!["--compatible", "--incompatible"],
                vec!["--lockfile-only", "--incompatible"],
                vec!["--no-lockfile", "--lockfile", "refresh"],
                vec!["--no-lockfile", "--resolve-lockfile"],
                vec!["--max-concurrency", "0"],
                vec!["--timeout-seconds", "0"],
            ] {
                let mut command = vec!["clippier", "upgrade"];
                command.extend(args);
                assert!(crate::Args::try_parse_from(command).is_err());
            }
        }

        #[test]
        fn maps_explicit_policies_and_registry_limits() {
            let command = parse(&[
                "clippier",
                "upgrade",
                "--incompatible",
                "--pins",
                "upgrade",
                "--prerelease",
                "allow",
                "--source",
                "include-path",
                "--lockfile",
                "resolve",
                "--resolve-lockfile",
                "--config",
                "custom.toml",
                "--manifest-path",
                "Cargo.toml",
                "--msrv",
                "1.85.0",
                "--timeout-seconds",
                "7",
                "--max-retries",
                "0",
                "--max-concurrency",
                "2",
            ]);
            let options = command.flags.options();
            assert_eq!(options.policy, Some(UpgradePolicy::Incompatible));
            assert_eq!(options.pinned, Some(PinnedPolicy::Upgrade));
            assert_eq!(options.prerelease, Some(PrereleasePolicy::Allow));
            assert_eq!(options.source, Some(SourcePolicy::IncludePath));
            assert_eq!(options.lockfile, Some(LockfilePolicy::Resolve));
            assert!(options.resolve_lockfile);
            assert_eq!(options.config_path, Some(PathBuf::from("custom.toml")));
            assert_eq!(options.manifest_path, Some(PathBuf::from("Cargo.toml")));
            assert_eq!(options.msrv.as_deref(), Some("1.85.0"));
            assert_eq!(options.registry.timeout_seconds, 7);
            assert_eq!(options.registry.max_retries, 0);
            assert_eq!(options.registry.max_concurrency, 2);
        }

        #[test]
        fn confirmation_accepts_only_yes_and_reads_once() {
            for answer in ["y\n", "yes\n", "Y\n", "YeS\n", " yes \n"] {
                assert!(read_confirmation(&mut Cursor::new(answer)).unwrap());
            }
            for answer in ["", "\n", "n\n", "no\n", "sure\n", "yes please\n", "n\ny\n"] {
                assert!(!read_confirmation(&mut Cursor::new(answer)).unwrap());
            }
            let mut input = Cursor::new("n\nyes\n");
            assert!(!read_confirmation(&mut input).unwrap());
            assert_eq!(input.position(), 2);
        }
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[cfg(feature = "versioning")]
#[derive(Subcommand)]
enum VersionCommands {
    /// Bump workspace package versions by a semantic version component
    Bump {
        /// Version component to bump
        #[arg(value_enum)]
        kind: VersionBump,

        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 2, default_value = ".")]
        workspace_root: PathBuf,

        /// Only bump the selected package(s)
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Only include publishable packages
        #[arg(long)]
        publishable_only: bool,

        /// Prerelease identifier for prerelease bumps
        #[arg(long)]
        pre: Option<String>,

        /// Print the bump plan without writing files
        #[arg(long)]
        dry_run: bool,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
    },
    /// Set workspace package versions to an exact version
    Set {
        /// Exact version to set
        version: String,

        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 2, default_value = ".")]
        workspace_root: PathBuf,

        /// Only set the selected package(s)
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Only include publishable packages
        #[arg(long)]
        publishable_only: bool,

        /// Print the bump plan without writing files
        #[arg(long)]
        dry_run: bool,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
    },
}

#[cfg(feature = "publish")]
#[derive(Subcommand)]
enum ConfigCommands {
    /// Show effective preferences and where each value came from
    Show {
        /// Path to the workspace root or Cargo.toml
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// Output format
        #[arg(long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
    },
}

#[cfg(feature = "release")]
#[derive(Subcommand)]
enum ReleaseCommands {
    /// Reconstruct release requirements from workspace and crates.io
    Status {
        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 1, default_value = ".")]
        workspace_root: PathBuf,

        /// Only inspect the selected publishable package(s)
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Optional Git base revision used only to narrow registry-backed candidates
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,

        /// Optional Git head revision paired with --git-base
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,

        /// Override cargo-semver-checks feature selection
        #[arg(long, value_enum)]
        semver_feature_policy: Option<clippier::release::SemverFeaturePolicy>,

        /// Features enabled for both current and baseline analyses
        #[arg(long, value_delimiter = ',')]
        semver_features: Vec<String>,

        /// Features enabled only for current source
        #[arg(long, value_delimiter = ',')]
        semver_current_features: Vec<String>,

        /// Features enabled only for registry baseline
        #[arg(long, value_delimiter = ',')]
        semver_baseline_features: Vec<String>,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
    },
    /// Apply reconstructed versions and dependency requirements
    Prepare {
        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 1, default_value = ".")]
        workspace_root: PathBuf,
        /// Only prepare the selected publishable package(s)
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Optional Git base revision used only to narrow registry-backed candidates
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,

        /// Optional Git head revision paired with --git-base
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,

        /// Override cargo-semver-checks feature selection
        #[arg(long, value_enum)]
        semver_feature_policy: Option<clippier::release::SemverFeaturePolicy>,

        /// Features enabled for both current and baseline analyses
        #[arg(long, value_delimiter = ',')]
        semver_features: Vec<String>,

        /// Features enabled only for current source
        #[arg(long, value_delimiter = ',')]
        semver_current_features: Vec<String>,

        /// Features enabled only for registry baseline
        #[arg(long, value_delimiter = ',')]
        semver_baseline_features: Vec<String>,
        /// Print required edits without writing files
        #[arg(long)]
        dry_run: bool,
        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
    },
    /// Verify that current manifests fully materialize reconstructed release requirements
    Verify {
        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 1, default_value = ".")]
        workspace_root: PathBuf,
        /// Only verify the selected publishable package(s)
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Optional Git base revision used only to narrow registry-backed candidates
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,

        /// Optional Git head revision paired with --git-base
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,

        /// Override cargo-semver-checks feature selection
        #[arg(long, value_enum)]
        semver_feature_policy: Option<clippier::release::SemverFeaturePolicy>,

        /// Features enabled for both current and baseline analyses
        #[arg(long, value_delimiter = ',')]
        semver_features: Vec<String>,

        /// Features enabled only for current source
        #[arg(long, value_delimiter = ',')]
        semver_current_features: Vec<String>,

        /// Features enabled only for registry baseline
        #[arg(long, value_delimiter = ',')]
        semver_baseline_features: Vec<String>,
        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
    },
    /// Publish the verified pending release closure
    Publish {
        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 1, default_value = ".")]
        workspace_root: PathBuf,
        /// Only publish selected roots plus mandatory pending dependencies
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Optional Git base revision used only to narrow registry-backed candidates
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,

        /// Optional Git head revision paired with --git-base
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,

        /// Override cargo-semver-checks feature selection
        #[arg(long, value_enum)]
        semver_feature_policy: Option<clippier::release::SemverFeaturePolicy>,

        /// Features enabled for both current and baseline analyses
        #[arg(long, value_delimiter = ',')]
        semver_features: Vec<String>,

        /// Features enabled only for current source
        #[arg(long, value_delimiter = ',')]
        semver_current_features: Vec<String>,

        /// Features enabled only for registry baseline
        #[arg(long, value_delimiter = ',')]
        semver_baseline_features: Vec<String>,
        /// Package and resolve pending crates without uploading (use --verify to compile)
        #[arg(long)]
        dry_run: bool,
        /// Stop publication preflight after its first failure
        #[arg(long)]
        fail_fast: bool,
        /// Run Cargo package verification
        #[arg(long)]
        verify: bool,
        /// Allow unrelated dirty workspace files
        #[arg(long)]
        allow_dirty: bool,
        /// Package purity policy (overrides configuration)
        #[arg(long, value_enum)]
        purity: Option<PurityPolicy>,
        /// Require packaged contents to match the published source
        #[arg(long, conflicts_with = "purity")]
        require_pure: bool,
        /// Seconds to wait for registry availability
        #[arg(long, default_value_t = 300)]
        publish_timeout_secs: u64,
        /// Seconds between registry availability checks
        #[arg(long, default_value_t = 10)]
        publish_poll_secs: u64,
        /// Retries after registry rate limiting
        #[arg(long, default_value_t = 3)]
        rate_limit_retries: u16,
        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
        /// Color mode for Cargo output
        #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
        color: ColorMode,
    },
    /// Materialize inherited versions for independently publishable packages
    Independentize {
        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 1, default_value = ".")]
        workspace_root: PathBuf,

        /// Only migrate the selected publishable package(s)
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Print the migration without writing manifests
        #[arg(long)]
        dry_run: bool,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze unused Cargo dependencies and preview or apply removals
    #[cfg(feature = "machete")]
    Machete(machete_cli::MacheteCli),
    /// Upgrade dependency requirements and the lockfile (plan first, then confirm)
    #[cfg(feature = "upgrade")]
    Upgrade(upgrade_cli::UpgradeCli),
    /// Run the bundled Markdown formatter
    #[cfg(feature = "md")]
    Md {
        #[command(subcommand)]
        command: clippier_md::cli::Commands,
    },
    /// Guide setup of clippier.toml in the current directory
    #[cfg(feature = "_tools")]
    Init {
        /// Save wizard defaults without prompts or terminal interaction
        #[arg(long)]
        non_interactive: bool,
        /// Enable tools (comma-separated; repeatable)
        #[arg(long, value_delimiter = ',', requires = "non_interactive")]
        enable: Vec<String>,
        /// Disable tools (comma-separated; repeatable)
        #[arg(long, value_delimiter = ',', requires = "non_interactive")]
        disable: Vec<String>,
        /// Override capabilities: tool=format,lint (repeatable)
        #[arg(long, requires = "non_interactive")]
        capabilities: Vec<String>,
        /// Override formatter scope: tool=js,ts (repeatable)
        #[arg(long, requires = "non_interactive")]
        format_extensions: Vec<String>,
        /// Require selected tools: true/false; default preserves existing requirements
        #[arg(long, requires = "non_interactive", action = clap::ArgAction::Set)]
        required: Option<bool>,
    },
    Dependencies {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        features: Option<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    Environment {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        features: Option<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    CiSteps {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        features: Option<String>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    Features {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        #[arg(long)]
        offset: Option<u16>,

        #[arg(long)]
        max: Option<u16>,

        #[arg(long)]
        max_parallel: Option<u16>,

        #[arg(long)]
        chunked: Option<u16>,

        #[arg(short, long)]
        spread: bool,

        /// Randomize features before chunking/spreading (useful for CI to test different feature combinations)
        #[arg(long)]
        randomize: bool,

        /// Seed for randomization (enables deterministic randomization when provided)
        #[arg(long)]
        seed: Option<u64>,

        #[arg(long)]
        features: Option<String>,

        #[arg(long)]
        skip_features: Option<String>,

        #[arg(long)]
        required_features: Option<String>,

        /// List of changed files (paths relative to workspace root) - only include affected packages
        #[arg(long, value_delimiter = ',')]
        changed_files: Option<Vec<String>>,

        /// Git base commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,

        /// Git head commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,

        /// Include reasoning for why each package is affected in the JSON output (only works with --changed-files)
        #[arg(long)]
        include_reasoning: bool,

        /// List of specific packages to process (comma-separated)
        #[arg(long, value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Glob patterns to ignore when detecting affected packages (e.g., "**/*.md", "*.txt")
        /// Can be specified multiple times. Use "!" prefix for negation (e.g., "!important.md")
        #[arg(long, action = clap::ArgAction::Append)]
        ignore: Vec<String>,

        /// Skip packages matching criteria (format: property<op>value, e.g., "package.publish=false")
        /// Can be specified multiple times. ANY match causes package to be skipped.
        #[arg(long, action = clap::ArgAction::Append)]
        skip_if: Vec<String>,

        /// Only include packages matching criteria (format: property<op>value, e.g., "categories@=audio")
        /// Can be specified multiple times. ALL criteria must match (AND logic between properties).
        #[arg(long, action = clap::ArgAction::Append)]
        include_if: Vec<String>,

        /// Lua transform scripts to apply to the generated matrix (can be specified multiple times)
        #[cfg(feature = "_transforms")]
        #[arg(long, action = clap::ArgAction::Append)]
        transform_scripts: Vec<PathBuf>,

        /// Enable trace mode for transform debugging
        #[cfg(feature = "_transforms")]
        #[arg(long)]
        transform_trace: bool,

        /// Workspace type(s) to use. Can be specified multiple times.
        /// If not specified, all workspace types are detected and the highest priority (cargo) is used.
        #[cfg(feature = "_workspace")]
        #[arg(long, value_enum)]
        workspace_type: Option<Vec<clippier::workspace::WorkspaceType>>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
    WorkspaceDeps {
        /// Path to the workspace root
        workspace_root: PathBuf,
        /// Name of the target package
        package: String,
        /// Features to enable (optional)
        #[arg(long)]
        features: Option<Vec<String>>,
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
        /// Include all potential workspace dependencies, regardless of feature activation
        #[arg(long)]
        all_potential_deps: bool,
    },
    GenerateDockerfile {
        /// Path to the workspace root OR git URL
        workspace_root: PathBuf,
        /// Name of the target package to build
        package: String,
        /// Git reference (branch/tag/commit) when using git URL
        #[arg(long, default_value = "master")]
        git_ref: String,
        /// Features to enable for the target package (optional)
        #[arg(long)]
        features: Option<Vec<String>>,
        /// Do not activate the `default` feature
        #[arg(long)]
        no_default_features: bool,
        /// Output path for the generated Dockerfile
        #[arg(long)]
        output: PathBuf,
        /// Docker base image for the builder stage
        #[arg(long, default_value = "rust:1-bookworm")]
        base_image: String,
        /// Docker base image for the final stage
        #[arg(long, default_value = "debian:bookworm-slim")]
        final_image: String,
        /// Arguments to pass to the binary in the CMD instruction
        #[arg(long, action = clap::ArgAction::Append)]
        arg: Vec<String>,
        /// Build arguments to pass to cargo build
        #[arg(long)]
        build_args: Option<String>,
        /// Generate dockerignore file alongside Dockerfile
        #[arg(long, default_value = "true")]
        generate_dockerignore: bool,
        /// Environment variables to include in the generated Dockerfile (format: KEY=VALUE)
        #[arg(long, action = clap::ArgAction::Append)]
        env: Vec<String>,
        /// Environment variables to set during the build process (format: KEY=VALUE)
        #[arg(long, action = clap::ArgAction::Append)]
        build_env: Vec<String>,
        /// Specify the binary name to build and use in the Dockerfile (overrides automatic detection)
        #[arg(long)]
        bin: Option<String>,
    },
    AffectedPackages {
        /// Path to the workspace root
        workspace_root: PathBuf,
        /// List of changed files (paths relative to workspace root)
        #[arg(long, value_delimiter = ',')]
        changed_files: Vec<String>,
        /// Package to check if affected (optional - if not provided, returns all affected packages)
        #[arg(long)]
        target_package: Option<String>,
        /// Git base commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,
        /// Git head commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,
        /// Include reasoning for why each package is affected in the JSON output
        #[arg(long)]
        include_reasoning: bool,
        /// Glob patterns to ignore when detecting affected packages (e.g., "**/*.md", "*.txt")
        /// Can be specified multiple times. Use "!" prefix for negation (e.g., "!important.md")
        #[arg(long, action = clap::ArgAction::Append)]
        ignore: Vec<String>,
        /// Workspace type(s) to use. Can be specified multiple times.
        /// If not specified, all workspace types are detected and the highest priority (cargo) is used.
        #[cfg(feature = "_workspace")]
        #[arg(long, value_enum)]
        workspace_type: Option<Vec<clippier::workspace::WorkspaceType>>,
        /// Output format
        #[arg(long, value_enum, default_value_t=OutputType::Json)]
        output: OutputType,
    },
    ValidateFeaturePropagation {
        /// Features to validate (comma-separated, e.g., "fail-on-warnings,cpal")
        /// If not specified, validates all matching features
        #[arg(long, value_delimiter = ',')]
        features: Option<Vec<String>>,

        /// Features to skip during validation (comma-separated, supports glob patterns)
        /// Supports wildcards (* and ?) and negation (! prefix)
        /// Examples: "default,test-*", "*-codec", "*,!fail-on-warnings"
        /// If not specified, defaults to skipping "default" feature and features starting with "_"
        /// Use empty string to skip nothing: --skip-features ""
        #[arg(long, value_delimiter = ',')]
        skip_features: Option<Vec<String>>,

        /// Path to package or workspace (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Only validate workspace packages (ignore external dependencies)
        #[arg(long, default_value_t = true)]
        workspace_only: bool,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,

        /// Exit with error code if validation fails (for CI)
        #[arg(long, default_value_t = true)]
        fail_on_error: bool,

        /// Require strict optional dependency propagation syntax
        /// When enabled, optional dependencies MUST use `dep?/feature` syntax
        /// When disabled (default), accepts both `dep?/feature` and `dep/feature`
        #[arg(long, default_value_t = false)]
        strict_optional: bool,

        /// Allow specific missing propagations (format: "[package:]feature:dependency")
        /// Can be specified multiple times. Package is optional and defaults to all packages.
        /// Examples: `"fail-on-warnings:tcp"`, `"server:async:sync_dep"`
        #[arg(long, action = clap::ArgAction::Append)]
        allow_missing: Vec<String>,

        /// Allow specific incorrect propagations (format: "[package:]feature:entry")
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        allow_incorrect: Vec<String>,

        /// Suppress all validation for specific packages (supports wildcards)
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        ignore_package: Vec<String>,

        /// Suppress validation for specific features globally (supports wildcards)
        /// Can be specified multiple times
        #[arg(long, action = clap::ArgAction::Append)]
        ignore_feature: Vec<String>,

        /// Load overrides from clippier.toml configuration files
        #[arg(long, default_value_t = true)]
        use_config_overrides: bool,

        /// Load overrides from Cargo.toml metadata
        #[arg(long, default_value_t = true)]
        use_cargo_metadata_overrides: bool,

        /// Warn about expired overrides
        #[arg(long, default_value_t = true)]
        warn_expired: bool,

        /// Fail validation if expired overrides exist
        #[arg(long, default_value_t = false)]
        fail_on_expired: bool,

        /// Show verbose override information
        #[arg(long, default_value_t = false)]
        verbose_overrides: bool,

        /// Packages to validate as parent packages (comma-separated)
        /// For these packages, validates that ALL features from their workspace
        /// dependencies are exposed with a prefix pattern.
        /// Can also be configured in clippier.toml files.
        #[arg(long, value_delimiter = ',')]
        parent_packages: Option<Vec<String>>,

        /// Maximum depth for nested dependency feature checking
        /// If not specified, follows full dependency chain (no limit)
        #[arg(long)]
        parent_depth: Option<u8>,

        /// Additional features to skip when validating parent packages (comma-separated)
        /// Added to the default skip list `["default", "_*"]`
        #[arg(long, value_delimiter = ',')]
        parent_skip_features: Option<Vec<String>>,

        /// Feature prefix pattern for parent packages
        /// Format: `dep_name:prefix` (e.g., `switchy_database:database`)
        /// If not specified, prefix is inferred from dependency name
        #[arg(long, action = clap::ArgAction::Append)]
        parent_prefix: Vec<String>,

        /// Disable loading parent config from clippier.toml files
        #[arg(long, default_value_t = false)]
        no_parent_config: bool,
    },
    Packages {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        os: Option<String>,

        /// List of specific packages to process (comma-separated)
        #[arg(long, value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// List of changed files (paths relative to workspace root) - only include affected packages
        #[arg(long, value_delimiter = ',')]
        changed_files: Option<Vec<String>>,

        /// Git base commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_base: Option<String>,

        /// Git head commit for external dependency analysis (requires git-diff feature)
        #[cfg(feature = "git-diff")]
        #[arg(long)]
        git_head: Option<String>,

        /// Include reasoning for why each package is affected in the JSON output
        #[arg(long)]
        include_reasoning: bool,

        /// Maximum number of packages in matrix
        #[arg(long)]
        max_parallel: Option<u16>,

        /// Glob patterns to ignore when detecting affected packages (e.g., "**/*.md", "*.txt")
        /// Can be specified multiple times. Use "!" prefix for negation (e.g., "!important.md")
        #[arg(long, action = clap::ArgAction::Append)]
        ignore: Vec<String>,

        /// Skip packages matching criteria (format: property<op>value, e.g., "package.publish=false")
        /// Can be specified multiple times. ANY match causes package to be skipped.
        #[arg(long, action = clap::ArgAction::Append)]
        skip_if: Vec<String>,

        /// Only include packages matching criteria (format: property<op>value, e.g., "categories@=audio")
        /// Can be specified multiple times. ALL criteria must match (AND logic between properties).
        #[arg(long, action = clap::ArgAction::Append)]
        include_if: Vec<String>,

        /// Workspace type(s) to use. Can be specified multiple times.
        /// If not specified, all workspace types are detected and the highest priority (cargo) is used.
        #[cfg(feature = "_workspace")]
        #[arg(long, value_enum)]
        workspace_type: Option<Vec<clippier::workspace::WorkspaceType>>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Json)]
        output: OutputType,
    },
    /// Prepare and publish stateless Cargo releases
    #[cfg(feature = "release")]
    Release {
        #[command(subcommand)]
        command: ReleaseCommands,
    },
    /// Bump Cargo workspace package versions
    #[cfg(feature = "versioning")]
    Version {
        #[command(subcommand)]
        command: VersionCommands,
    },
    /// Inspect effective Clippier preferences
    #[cfg(feature = "publish")]
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Publish Cargo workspace crates to crates.io in dependency order
    #[cfg(feature = "publish")]
    Publish {
        /// Path to workspace root or Cargo.toml. Defaults to current directory.
        #[arg(index = 1, default_value = ".")]
        workspace_root: PathBuf,

        /// Only publish the selected package(s), plus their normal/build workspace dependencies
        #[arg(short = 'p', long = "package", value_delimiter = ',')]
        packages: Option<Vec<String>>,

        /// Package and resolve all pending crates without uploading (use --verify to compile)
        #[arg(long)]
        dry_run: bool,

        /// Stop publication preflight after its first failure
        #[arg(long)]
        fail_fast: bool,

        /// Compile packaged contents during preflight and verify again before upload
        #[arg(long)]
        verify: bool,

        /// Pass `--allow-dirty` to `cargo publish`
        #[arg(long)]
        allow_dirty: bool,

        /// Package purity policy (overrides configuration)
        #[arg(long, value_enum)]
        purity: Option<PurityPolicy>,

        /// Require packaged contents to match the published source
        #[arg(long, conflicts_with = "purity")]
        require_pure: bool,

        /// Seconds to wait for each newly-published crate to appear on crates.io
        #[arg(long, default_value_t = 300)]
        publish_timeout_secs: u64,

        /// Seconds between crates.io availability checks
        #[arg(long, default_value_t = 10)]
        publish_poll_secs: u64,

        /// Number of times to retry a package after crates.io rate limiting
        #[arg(long, default_value_t = 3)]
        rate_limit_retries: u16,

        /// Output format
        #[arg(short, long, value_enum, default_value_t = OutputType::Raw)]
        output: OutputType,

        /// Color mode for cargo publish output
        #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
        color: ColorMode,
    },
    /// Aggregate toolchains and dependencies from all workspace packages for CI setup
    WorkspaceToolchains {
        /// Path to the workspace root
        #[arg(index = 1, default_value = ".")]
        workspace_root: PathBuf,

        /// Target operating system (ubuntu, windows, macos)
        #[arg(long)]
        os: String,

        /// Output format
        #[arg(short, long, value_enum, default_value_t=OutputType::Json)]
        output: OutputType,
    },
    /// Run linters and format checkers
    #[cfg(feature = "check")]
    Check {
        /// Working directory to run in
        #[arg(short, long)]
        working_dir: Option<PathBuf>,

        /// Specific tools to run (comma-separated)
        #[arg(long, value_delimiter = ',')]
        tools: Option<Vec<String>>,

        /// List available tools instead of running them
        #[arg(long)]
        list: bool,

        /// Tools that MUST be installed (error if missing)
        #[arg(long, value_delimiter = ',')]
        required: Option<Vec<String>>,

        /// Tools to skip even if detected
        #[arg(long, value_delimiter = ',')]
        skip: Option<Vec<String>>,

        /// Output format
        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,

        /// Color mode for tool output
        #[arg(long, value_enum, default_value_t=ColorMode::Auto)]
        color: ColorMode,

        /// Disable real-time pane TUI output
        #[arg(long, default_value_t = false)]
        no_tui: bool,

        /// Use the full-screen live view instead of compact inline output
        #[arg(long, conflicts_with = "no_tui")]
        tui_fullscreen: bool,

        /// Include binary files normally excluded from file-oriented tools
        #[arg(long)]
        include_binary: bool,

        /// Disable package-manager runner fallback (bunx/pnpm/npx)
        #[arg(long, default_value_t = false)]
        no_runner_fallback: bool,

        /// Override tool executable path (repeatable: --tool-path key=value)
        #[arg(long = "tool-path", action = clap::ArgAction::Append)]
        tool_path: Vec<String>,

        /// Force biome to use .editorconfig
        #[arg(long, default_value_t = false)]
        biome_use_editorconfig: bool,

        /// Disable biome .editorconfig support
        #[arg(long, default_value_t = false)]
        no_biome_use_editorconfig: bool,

        /// Force biome VCS ignore semantics
        #[arg(long, default_value_t = false)]
        biome_use_vcs_ignore: bool,

        /// Disable biome VCS ignore semantics
        #[arg(long, default_value_t = false)]
        no_biome_use_vcs_ignore: bool,
    },
    /// Run formatters
    #[cfg(feature = "format")]
    Fmt {
        /// Working directory to run in
        #[arg(short, long)]
        working_dir: Option<PathBuf>,

        /// Only check formatting without modifying files
        #[arg(long)]
        check: bool,

        /// Files to format: changed Git files, branch changes, or all files
        #[arg(long, value_enum)]
        scope: Option<FormatScope>,

        /// Base revision for --scope branch
        #[arg(long)]
        git_base: Option<String>,

        /// Specific tools to run (comma-separated)
        #[arg(long, value_delimiter = ',')]
        tools: Option<Vec<String>>,

        /// List available tools instead of running them
        #[arg(long)]
        list: bool,

        /// Tools that MUST be installed (error if missing)
        #[arg(long, value_delimiter = ',')]
        required: Option<Vec<String>>,

        /// Tools to skip even if detected
        #[arg(long, value_delimiter = ',')]
        skip: Option<Vec<String>>,

        /// Output format
        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,

        /// Color mode for tool output
        #[arg(long, value_enum, default_value_t=ColorMode::Auto)]
        color: ColorMode,

        /// Disable real-time pane TUI output
        #[arg(long, default_value_t = false)]
        no_tui: bool,

        /// Use the full-screen live view instead of compact inline output
        #[arg(long, conflicts_with = "no_tui")]
        tui_fullscreen: bool,

        /// Include binary files normally excluded from file-oriented tools
        #[arg(long)]
        include_binary: bool,

        /// Disable package-manager runner fallback (bunx/pnpm/npx)
        #[arg(long, default_value_t = false)]
        no_runner_fallback: bool,

        /// Override tool executable path (repeatable: --tool-path key=value)
        #[arg(long = "tool-path", action = clap::ArgAction::Append)]
        tool_path: Vec<String>,

        /// Force biome to use .editorconfig
        #[arg(long, default_value_t = false)]
        biome_use_editorconfig: bool,

        /// Disable biome .editorconfig support
        #[arg(long, default_value_t = false)]
        no_biome_use_editorconfig: bool,

        /// Force biome VCS ignore semantics
        #[arg(long, default_value_t = false)]
        biome_use_vcs_ignore: bool,

        /// Disable biome VCS ignore semantics
        #[arg(long, default_value_t = false)]
        no_biome_use_vcs_ignore: bool,
    },
}

#[switchy_async::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_lines, clippy::future_not_send)]
async fn run() -> Result<(), BoxError> {
    let paths =
        moosicbox_log_runtime::resolve_paths(&moosicbox_log_runtime::LogRuntimePathsConfig {
            app_name: "moosicbox",
            state_dir_env: "MOOSICBOX_STATE_DIR",
            log_dir_env: "MOOSICBOX_LOG_DIR",
        });
    let mut log_config = moosicbox_log_runtime::init::InitConfig::new(&paths);
    log_config.source_mode = moosicbox_log_runtime::init::SourceMode::Both;
    let _log_handle =
        moosicbox_log_runtime::init::init(log_config).expect("Failed to initialize logging");

    let args = Args::parse();

    let result = match args.cmd {
        #[cfg(feature = "machete")]
        Commands::Machete(command) => {
            let exit_code = machete_cli::run(&command)?;
            if exit_code != 0 {
                std::process::exit(i32::from(exit_code));
            }
            return Ok(());
        }
        #[cfg(feature = "upgrade")]
        Commands::Upgrade(command) => {
            let exit_code = upgrade_cli::run(&command).await?;
            if exit_code != 0 {
                std::process::exit(i32::from(exit_code));
            }
            return Ok(());
        }
        #[cfg(feature = "md")]
        Commands::Md { command } => {
            let exit_code = clippier_md::cli::run(&command)?;
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
            String::new()
        }
        #[cfg(feature = "_tools")]
        Commands::Init {
            non_interactive,
            enable,
            disable,
            capabilities,
            format_extensions,
            required,
        } => {
            let root = std::env::current_dir()?;
            if non_interactive {
                clippier::tools::init::initialize_non_interactive(
                    &root,
                    &clippier::tools::init::InitOptions {
                        enable,
                        disable,
                        capabilities,
                        format_extensions,
                        required,
                    },
                    &mut std::io::stdout().lock(),
                )?;
            } else {
                clippier::tools::init::initialize_terminal(&root)?;
            }
            String::new()
        }
        Commands::Dependencies {
            file,
            os,
            features,
            output,
        } => handle_dependencies_command(&file, os.as_deref(), features.as_deref(), output).await?,
        Commands::Environment {
            file,
            os,
            features,
            output,
        } => handle_environment_command(&file, os.as_deref(), features.as_deref(), output).await?,
        Commands::CiSteps {
            file,
            os,
            features,
            output,
        } => handle_ci_steps_command(&file, os.as_deref(), features.as_deref(), output).await?,
        Commands::Features {
            file,
            os,
            offset,
            max,
            max_parallel,
            chunked,
            spread,
            randomize,
            seed,
            features,
            skip_features,
            required_features,
            changed_files,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            include_reasoning,
            packages,
            ignore,
            skip_if,
            include_if,
            #[cfg(feature = "_transforms")]
            transform_scripts,
            #[cfg(feature = "_transforms")]
            transform_trace,
            #[cfg(feature = "_workspace")]
            workspace_type,
            output,
        } => {
            handle_features_command(
                &file,
                os.as_deref(),
                offset,
                max,
                max_parallel,
                chunked,
                spread,
                randomize,
                seed,
                features.as_deref(),
                skip_features.as_deref(),
                required_features.as_deref(),
                packages.as_deref(),
                changed_files.as_deref(),
                #[cfg(feature = "git-diff")]
                git_base.as_deref(),
                #[cfg(feature = "git-diff")]
                git_head.as_deref(),
                include_reasoning,
                if ignore.is_empty() {
                    None
                } else {
                    Some(&ignore)
                },
                &skip_if,
                &include_if,
                #[cfg(feature = "_transforms")]
                &transform_scripts,
                #[cfg(feature = "_transforms")]
                transform_trace,
                #[cfg(feature = "_workspace")]
                workspace_type.as_deref(),
                output,
            )
            .await?
        }
        Commands::WorkspaceDeps {
            workspace_root,
            package,
            features,
            format,
            all_potential_deps,
        } => handle_workspace_deps_command(
            &workspace_root,
            &package,
            features.as_deref(),
            &format,
            all_potential_deps,
        )?,
        Commands::GenerateDockerfile {
            workspace_root,
            package,
            git_ref,
            features,
            no_default_features,
            output,
            base_image,
            final_image,
            arg,
            build_args,
            generate_dockerignore,
            env,
            build_env,
            bin,
        } => {
            handle_generate_dockerfile_command(
                &workspace_root,
                &package,
                &git_ref,
                features.as_deref(),
                no_default_features,
                &output,
                &base_image,
                &final_image,
                &arg,
                build_args.as_deref(),
                generate_dockerignore,
                &env,
                &build_env,
                bin.as_deref(),
            )
            .await?
        }
        Commands::AffectedPackages {
            workspace_root,
            changed_files,
            target_package,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            include_reasoning,
            ignore,
            #[cfg(feature = "_workspace")]
            workspace_type,
            output,
        } => {
            handle_affected_packages_command(
                &workspace_root,
                &changed_files,
                target_package.as_deref(),
                #[cfg(feature = "git-diff")]
                git_base.as_deref(),
                #[cfg(feature = "git-diff")]
                git_head.as_deref(),
                include_reasoning,
                if ignore.is_empty() {
                    None
                } else {
                    Some(&ignore)
                },
                #[cfg(feature = "_workspace")]
                workspace_type.as_deref(),
                output,
            )
            .await?
        }
        Commands::ValidateFeaturePropagation {
            features,
            skip_features,
            path,
            workspace_only,
            output,
            fail_on_error,
            strict_optional,
            allow_missing,
            allow_incorrect,
            ignore_package,
            ignore_feature,
            use_config_overrides,
            use_cargo_metadata_overrides,
            warn_expired,
            fail_on_expired,
            verbose_overrides,
            parent_packages,
            parent_depth,
            parent_skip_features,
            parent_prefix,
            no_parent_config,
        } => {
            let result = handle_validate_feature_propagation_command(
                features,
                skip_features,
                path,
                workspace_only,
                output,
                strict_optional,
                &allow_missing,
                &allow_incorrect,
                &ignore_package,
                &ignore_feature,
                use_config_overrides,
                use_cargo_metadata_overrides,
                warn_expired,
                fail_on_expired,
                verbose_overrides,
                parent_packages,
                parent_depth,
                parent_skip_features,
                &parent_prefix,
                no_parent_config,
            )?;

            match output {
                OutputType::Raw => print_human_output(&result),
                OutputType::Json => println!("{}", serde_json::to_string(&result)?),
            }

            if fail_on_error
                && (!result.errors.is_empty()
                    || !result
                        .parent_results
                        .iter()
                        .all(|r| r.missing_exposures.is_empty())
                    || (fail_on_expired
                        && result
                            .override_summary
                            .as_ref()
                            .is_some_and(|s| s.expired > 0)))
            {
                std::process::exit(1);
            }

            return Ok(()); // Early return since we handle output ourselves
        }
        Commands::Packages {
            file,
            os,
            packages,
            changed_files,
            #[cfg(feature = "git-diff")]
            git_base,
            #[cfg(feature = "git-diff")]
            git_head,
            #[cfg(feature = "git-diff")]
            include_reasoning,
            #[cfg(not(feature = "git-diff"))]
                include_reasoning: _,
            max_parallel,
            #[cfg(feature = "git-diff")]
            ignore,
            #[cfg(not(feature = "git-diff"))]
                ignore: _,
            skip_if,
            include_if,
            #[cfg(feature = "_workspace")]
            workspace_type,
            output,
        } => {
            handle_packages_command(
                &file,
                os.as_deref(),
                packages.as_deref(),
                changed_files.as_deref(),
                #[cfg(feature = "git-diff")]
                git_base.as_deref(),
                #[cfg(feature = "git-diff")]
                git_head.as_deref(),
                #[cfg(feature = "git-diff")]
                include_reasoning,
                max_parallel,
                #[cfg(feature = "git-diff")]
                Some(&ignore),
                &skip_if,
                &include_if,
                #[cfg(feature = "_workspace")]
                workspace_type.as_deref(),
                output,
            )
            .await?
        }
        #[cfg(feature = "release")]
        Commands::Release { command } => match command {
            ReleaseCommands::Status {
                workspace_root,
                packages,
                #[cfg(feature = "git-diff")]
                git_base,
                #[cfg(feature = "git-diff")]
                git_head,
                semver_feature_policy,
                semver_features,
                semver_current_features,
                semver_baseline_features,
                output,
            } => {
                handle_release_status_command(
                    &ReleaseStatusConfig {
                        workspace_root,
                        packages,
                        #[cfg(feature = "git-diff")]
                        git_base,
                        #[cfg(feature = "git-diff")]
                        git_head,
                        semver_feature_policy,
                        semver_features,
                        semver_current_features,
                        semver_baseline_features,
                    },
                    output,
                )
                .await?
            }
            ReleaseCommands::Prepare {
                workspace_root,
                packages,
                #[cfg(feature = "git-diff")]
                git_base,
                #[cfg(feature = "git-diff")]
                git_head,
                semver_feature_policy,
                semver_features,
                semver_current_features,
                semver_baseline_features,
                dry_run,
                output,
            } => {
                handle_release_prepare_command(
                    &ReleasePrepareConfig {
                        status: ReleaseStatusConfig {
                            workspace_root,
                            packages,
                            #[cfg(feature = "git-diff")]
                            git_base,
                            #[cfg(feature = "git-diff")]
                            git_head,
                            semver_feature_policy,
                            semver_features,
                            semver_current_features,
                            semver_baseline_features,
                        },
                        dry_run,
                    },
                    output,
                )
                .await?
            }
            ReleaseCommands::Verify {
                workspace_root,
                packages,
                #[cfg(feature = "git-diff")]
                git_base,
                #[cfg(feature = "git-diff")]
                git_head,
                semver_feature_policy,
                semver_features,
                semver_current_features,
                semver_baseline_features,
                output,
            } => {
                handle_release_verify_command(
                    &ReleaseStatusConfig {
                        workspace_root,
                        packages,
                        #[cfg(feature = "git-diff")]
                        git_base,
                        #[cfg(feature = "git-diff")]
                        git_head,
                        semver_feature_policy,
                        semver_features,
                        semver_current_features,
                        semver_baseline_features,
                    },
                    output,
                )
                .await?
            }
            ReleaseCommands::Publish {
                workspace_root,
                packages,
                #[cfg(feature = "git-diff")]
                git_base,
                #[cfg(feature = "git-diff")]
                git_head,
                semver_feature_policy,
                semver_features,
                semver_current_features,
                semver_baseline_features,
                dry_run,
                fail_fast,
                verify,
                allow_dirty,
                purity,
                require_pure,
                publish_timeout_secs,
                publish_poll_secs,
                rate_limit_retries,
                output,
                color,
            } => {
                handle_release_publish_command(
                    ReleasePublishConfig {
                        status: ReleaseStatusConfig {
                            workspace_root,
                            packages,
                            #[cfg(feature = "git-diff")]
                            git_base,
                            #[cfg(feature = "git-diff")]
                            git_head,
                            semver_feature_policy,
                            semver_features,
                            semver_current_features,
                            semver_baseline_features,
                        },
                        dry_run,
                        fail_fast,
                        verify,
                        allow_dirty,
                        purity: purity
                            .or_else(|| require_pure.then_some(PurityPolicy::RequirePure)),
                        color,
                        publish_timeout_secs,
                        publish_poll_secs,
                        rate_limit_retries,
                    },
                    output,
                )
                .await?
            }
            ReleaseCommands::Independentize {
                workspace_root,
                packages,
                dry_run,
                output,
            } => handle_independentize_command(
                &IndependentizeConfig {
                    workspace_root,
                    packages,
                    dry_run,
                },
                output,
            )?,
        },
        #[cfg(feature = "versioning")]
        Commands::Version { command } => match command {
            VersionCommands::Bump {
                kind,
                workspace_root,
                packages,
                publishable_only,
                pre,
                dry_run,
                output,
            } => {
                let config = VersionConfig {
                    workspace_root,
                    packages,
                    publishable_only,
                    dry_run,
                    operation: VersionOperation::Bump { kind, pre },
                };
                handle_version_command(&config, output)?
            }
            VersionCommands::Set {
                version,
                workspace_root,
                packages,
                publishable_only,
                dry_run,
                output,
            } => {
                let config = VersionConfig {
                    workspace_root,
                    packages,
                    publishable_only,
                    dry_run,
                    operation: VersionOperation::Set(version),
                };
                handle_version_command(&config, output)?
            }
        },
        #[cfg(feature = "publish")]
        Commands::Config { command } => match command {
            ConfigCommands::Show { workspace, output } => {
                let preferences = resolve_preferences(&workspace, PreferencesOverrides::default())?;
                match output {
                    OutputType::Raw => preferences.to_raw_string(),
                    OutputType::Json => serde_json::to_string_pretty(&preferences)?,
                }
            }
        },
        #[cfg(feature = "publish")]
        Commands::Publish {
            workspace_root,
            packages,
            dry_run,
            fail_fast,
            verify,
            allow_dirty,
            purity,
            require_pure,
            publish_timeout_secs,
            publish_poll_secs,
            rate_limit_retries,
            output,
            color,
        } => {
            let config = PublishConfig {
                workspace_root,
                packages,
                dry_run,
                fail_fast,
                verify,
                allow_dirty,
                purity: purity.or_else(|| require_pure.then_some(PurityPolicy::RequirePure)),
                color,
                publish_timeout: std::time::Duration::from_secs(publish_timeout_secs),
                publish_poll_interval: std::time::Duration::from_secs(publish_poll_secs),
                rate_limit_retries,
            };
            handle_publish_command(config, output).await?
        }
        Commands::WorkspaceToolchains {
            workspace_root,
            os,
            output,
        } => handle_workspace_toolchains_command(&workspace_root, &os, output)?,
        #[cfg(feature = "check")]
        Commands::Check {
            working_dir,
            tools,
            list,
            required,
            skip,
            output,
            color,
            no_tui,
            tui_fullscreen,
            include_binary,
            no_runner_fallback,
            tool_path,
            biome_use_editorconfig,
            no_biome_use_editorconfig,
            biome_use_vcs_ignore,
            no_biome_use_vcs_ignore,
        } => {
            if biome_use_editorconfig && no_biome_use_editorconfig {
                return Err(
                    "Cannot pass both --biome-use-editorconfig and --no-biome-use-editorconfig"
                        .into(),
                );
            }
            let biome_editorconfig_override = if biome_use_editorconfig {
                Some(true)
            } else if no_biome_use_editorconfig {
                Some(false)
            } else {
                None
            };
            if biome_use_vcs_ignore && no_biome_use_vcs_ignore {
                return Err(
                    "Cannot pass both --biome-use-vcs-ignore and --no-biome-use-vcs-ignore".into(),
                );
            }
            let biome_vcs_ignore_override = if biome_use_vcs_ignore {
                Some(true)
            } else if no_biome_use_vcs_ignore {
                Some(false)
            } else {
                None
            };
            let mut config = build_tools_config(
                working_dir.as_deref(),
                required.as_deref(),
                skip.as_deref(),
                tools.as_deref(),
                no_runner_fallback,
                &tool_path,
                biome_editorconfig_override,
                biome_vcs_ignore_override,
            )?;
            config.tui_fullscreen |= tui_fullscreen;
            if include_binary {
                config.scope.exclude_binary = false;
            }
            handle_check_command(
                working_dir.as_deref(),
                tools.as_deref(),
                list,
                config,
                output,
                color,
                !no_tui,
            )?
        }
        #[cfg(feature = "format")]
        Commands::Fmt {
            working_dir,
            check,
            scope,
            git_base,
            tools,
            list,
            required,
            skip,
            output,
            color,
            no_tui,
            tui_fullscreen,
            include_binary,
            no_runner_fallback,
            tool_path,
            biome_use_editorconfig,
            no_biome_use_editorconfig,
            biome_use_vcs_ignore,
            no_biome_use_vcs_ignore,
        } => {
            if biome_use_editorconfig && no_biome_use_editorconfig {
                return Err(
                    "Cannot pass both --biome-use-editorconfig and --no-biome-use-editorconfig"
                        .into(),
                );
            }
            let biome_editorconfig_override = if biome_use_editorconfig {
                Some(true)
            } else if no_biome_use_editorconfig {
                Some(false)
            } else {
                None
            };
            if biome_use_vcs_ignore && no_biome_use_vcs_ignore {
                return Err(
                    "Cannot pass both --biome-use-vcs-ignore and --no-biome-use-vcs-ignore".into(),
                );
            }
            let biome_vcs_ignore_override = if biome_use_vcs_ignore {
                Some(true)
            } else if no_biome_use_vcs_ignore {
                Some(false)
            } else {
                None
            };
            let mut config = build_tools_config(
                working_dir.as_deref(),
                required.as_deref(),
                skip.as_deref(),
                tools.as_deref(),
                no_runner_fallback,
                &tool_path,
                biome_editorconfig_override,
                biome_vcs_ignore_override,
            )?;
            apply_format_cli_overrides(&mut config, scope, git_base, !list)?;
            config.tui_fullscreen |= tui_fullscreen;
            if include_binary {
                config.scope.exclude_binary = false;
            }
            handle_fmt_command(
                working_dir.as_deref(),
                tools.as_deref(),
                check,
                list,
                config,
                output,
                color,
                !no_tui,
            )?
        }
    };

    if !result.is_empty() {
        println!("{result}");
    }

    Ok(())
}

#[cfg(all(test, feature = "publish"))]
mod publish_cli_tests {
    use super::*;

    fn publish_flags(arguments: &[&str]) -> (Option<PurityPolicy>, bool) {
        match Args::try_parse_from(arguments)
            .expect("valid publish arguments")
            .cmd
        {
            Commands::Publish {
                purity,
                require_pure,
                ..
            } => (purity, require_pure),
            #[cfg(feature = "release")]
            Commands::Release {
                command:
                    ReleaseCommands::Publish {
                        purity,
                        require_pure,
                        ..
                    },
            } => (purity, require_pure),
            _ => panic!("expected a publish command"),
        }
    }

    fn check_publish_purity_flags(command: &[&str]) {
        assert_eq!(publish_flags(command), (None, false));
        for (value, expected) in [
            ("warn", PurityPolicy::Warn),
            ("require-pure", PurityPolicy::RequirePure),
        ] {
            let mut arguments = command.to_vec();
            arguments.extend(["--purity", value]);
            assert_eq!(publish_flags(&arguments), (Some(expected), false));
        }
        let mut arguments = command.to_vec();
        arguments.push("--require-pure");
        assert_eq!(publish_flags(&arguments), (None, true));

        for flags in [
            ["--purity", "warn", "--require-pure"],
            ["--require-pure", "--purity", "require-pure"],
        ] {
            let mut arguments = command.to_vec();
            arguments.extend(flags);
            let error = Args::try_parse_from(arguments)
                .err()
                .expect("purity switches must conflict");
            assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
        }
        let mut arguments = command.to_vec();
        arguments.extend(["--purity", "invalid"]);
        assert_eq!(
            Args::try_parse_from(arguments)
                .err()
                .expect("unknown purity policy must be rejected")
                .kind(),
            clap::error::ErrorKind::InvalidValue
        );
    }

    #[test]
    fn publish_purity_flags() {
        check_publish_purity_flags(&["clippier", "publish"]);
    }

    #[cfg(feature = "release")]
    #[test]
    fn release_publish_purity_flags() {
        check_publish_purity_flags(&["clippier", "release", "publish"]);
    }

    #[test]
    fn config_show_defaults() {
        let args = Args::try_parse_from(["clippier", "config", "show"])
            .expect("config show accepts defaults");
        assert!(matches!(
            args.cmd,
            Commands::Config {
                command: ConfigCommands::Show { workspace, output: OutputType::Raw },
            } if workspace == std::path::Path::new(".")
        ));
    }

    #[test]
    fn config_show_workspace_and_output() {
        for output in ["raw", "json"] {
            let args = Args::try_parse_from([
                "clippier",
                "config",
                "show",
                "--workspace",
                ".",
                "--output",
                output,
            ])
            .expect("config show accepts workspace and output");
            let Commands::Config {
                command:
                    ConfigCommands::Show {
                        workspace,
                        output: parsed_output,
                    },
            } = args.cmd
            else {
                panic!("expected config show");
            };
            assert_eq!(workspace, PathBuf::from("."));
            assert!(matches!(
                (output, parsed_output),
                ("raw", OutputType::Raw) | ("json", OutputType::Json)
            ));
        }
        assert!(
            Args::try_parse_from(["clippier", "config", "show", "--output", "invalid",]).is_err()
        );
    }
}
