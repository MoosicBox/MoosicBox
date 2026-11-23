//! Checks that everything a switchy simulation links runs deterministically under it.
//!
//! A simulation is deterministic only if the code it runs reaches clocks, randomness, threads,
//! files, the environment and process-wide state through what the simulation controls (the
//! switchy crates). [`Check`] checks that for everything the simulation links, found from its
//! dependency graph rather than listed by hand:
//!
//! * **Local crates** (the workspace's, and other path dependencies) are parsed and scanned
//!   ([`scan`]), by the rules in each crate's manifest ([`manifest`]): which files run in
//!   simulations, which are native backends or the controlled versions themselves, and whether
//!   the crate runs inside a run at all. A crate without rules is scanned whole.
//! * **Crates from registries and git repositories** must each be on the simulation's audit
//!   list ([`config`]), with a verdict and a reason, as with `cargo vet`: one the list does not
//!   cover fails the check until it is audited.
//!
//! The configuration (shared forbidden patterns and the audit list) is the simulation
//! workspace's `determinism.toml`. The `switchy_cargo` binary runs the check:
//!
//! ```text
//! switchy_cargo --manifest-path simulation/Cargo.toml --package my-simulation
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod config;
pub mod graph;
pub mod manifest;
pub mod scan;

use std::{
    fmt::{self, Write as _},
    path::{Path, PathBuf},
};

pub use config::{Config, Verdict};
pub use manifest::Rules;
pub use scan::Finding;

/// A check of what one simulation links.
#[derive(Debug, Clone)]
pub struct Check {
    manifest_path: PathBuf,
    package: String,
    config: Config,
}

/// What a local crate's check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrateOutcome {
    /// Scanned: the lines that reach what no seed controls (none when it is clean).
    Scanned(Vec<Finding>),
    /// Not scanned: simulations do not run it inside a run, for this reason.
    NotRun(String),
    /// Its rules could not be used.
    BadRules(String),
}

/// What a [`Check`] found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Each local crate the simulation links, by name.
    pub crates: Vec<(String, CrateOutcome)>,
    /// External crates no audit entry covers (`name version (source)`).
    pub unaudited_missing: Vec<graph::ExternalCrate>,
    /// External crates covered only by an `unaudited` entry.
    pub unaudited: Vec<graph::ExternalCrate>,
    /// External crates covered by an audit with a verdict.
    pub audited: usize,
}

impl Check {
    /// A check of `package` in the simulation workspace at `manifest_path`, configured by
    /// `config` (the workspace's `determinism.toml`).
    #[must_use]
    pub fn new(manifest_path: impl Into<PathBuf>, package: impl Into<String>, config: Config) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            package: package.into(),
            config,
        }
    }

    /// A check configured by the `determinism.toml` beside `manifest_path` (an empty
    /// configuration if there is none).
    ///
    /// # Errors
    ///
    /// * If the configuration cannot be read or parsed
    pub fn with_workspace_config(
        manifest_path: impl Into<PathBuf>,
        package: impl Into<String>,
    ) -> Result<Self, String> {
        let manifest_path = manifest_path.into();
        let path = config_path(&manifest_path);
        let config = match std::fs::read_to_string(&path) {
            Ok(text) => Config::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        Ok(Self::new(manifest_path, package, config))
    }

    /// Run the check.
    ///
    /// # Errors
    ///
    /// * If the dependency graph cannot be resolved
    pub fn run(&self) -> Result<Report, String> {
        let graph = graph::resolve(&self.manifest_path, &self.package)?;
        let forbidden: Vec<String> = scan::FORBIDDEN
            .iter()
            .map(ToString::to_string)
            .chain(self.config.forbid.iter().cloned())
            .collect();
        let mut report = Report::default();
        for krate in &graph.local {
            let src = krate.dir.join("src");
            let outcome = match Rules::from_metadata(&krate.metadata, &src) {
                Err(e) => CrateOutcome::BadRules(e),
                Ok(rules) if !rules.runs => {
                    CrateOutcome::NotRun(rules.reason.unwrap_or_default())
                }
                Ok(rules) => match scan::scan(&src, &rules, &forbidden, &krate.features) {
                    Ok(findings) => CrateOutcome::Scanned(findings),
                    Err(e) => CrateOutcome::BadRules(format!("cannot read src/: {e}")),
                },
            };
            report.crates.push((krate.name.clone(), outcome));
        }
        for krate in graph.external {
            match self.config.audit(&krate.name, &krate.version).map(|a| a.verdict) {
                None => report.unaudited_missing.push(krate),
                Some(Verdict::Unaudited) => report.unaudited.push(krate),
                Some(_) => report.audited += 1,
            }
        }
        Ok(report)
    }
}

impl Check {
    /// Scan every external crate the simulation links by the shared rules (as if it had no
    /// rules of its own), as evidence for its audit: a crate whose compiled code reaches no
    /// clock, randomness, thread, file, environment or process-wide state is a candidate for
    /// `pure`.
    ///
    /// # Errors
    ///
    /// * If the dependency graph cannot be resolved
    pub fn scan_external(&self) -> Result<Vec<(graph::ExternalCrate, Vec<Finding>)>, String> {
        let graph = graph::resolve(&self.manifest_path, &self.package)?;
        let forbidden: Vec<String> = scan::FORBIDDEN
            .iter()
            .map(ToString::to_string)
            .chain(self.config.forbid.iter().cloned())
            .collect();
        Ok(graph
            .external
            .into_iter()
            .map(|krate| {
                let findings = scan::scan(
                    &krate.dir.join("src"),
                    &Rules::default(),
                    &forbidden,
                    &krate.features,
                )
                .unwrap_or_else(|e| {
                    vec![Finding {
                        file: String::new(),
                        line: 0,
                        what: format!("cannot read its sources: {e}"),
                    }]
                });
                (krate, findings)
            })
            .collect())
    }
}

/// The `determinism.toml` beside a workspace's `Cargo.toml`.
#[must_use]
pub fn config_path(manifest_path: &Path) -> PathBuf {
    manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("determinism.toml")
}

impl Report {
    /// Whether the check passed: every scanned crate clean, every rule usable, and every
    /// external crate on the audit list.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.unaudited_missing.is_empty()
            && self.crates.iter().all(|(_, outcome)| match outcome {
                CrateOutcome::Scanned(findings) => findings.is_empty(),
                CrateOutcome::NotRun(_) => true,
                CrateOutcome::BadRules(_) => false,
            })
    }

    /// Audit entries (verdict `unaudited`) for the external crates no entry covers, to add to
    /// the list as a baseline once they are accepted as they are; `reason` explains the
    /// baseline (when it was taken, say).
    #[must_use]
    pub fn baseline(&self, reason: &str) -> String {
        let mut text = String::new();
        for krate in &self.unaudited_missing {
            let _ = write!(
                text,
                "\n[[audit]]\nname = \"{}\"\nversion = \"={}\"\nverdict = \"unaudited\"\nreason = \"{}\"\n",
                krate.name,
                krate.version,
                reason.replace('"', "'")
            );
        }
        text
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (mut scanned, mut not_run, mut findings) = (0, 0, 0);
        for (name, outcome) in &self.crates {
            match outcome {
                CrateOutcome::Scanned(list) => {
                    scanned += 1;
                    findings += list.len();
                    if !list.is_empty() {
                        writeln!(f, "{name}: {} lines reach what no seed controls:", list.len())?;
                        for finding in list {
                            writeln!(f, "  {finding}")?;
                        }
                    }
                }
                CrateOutcome::NotRun(_) => not_run += 1,
                CrateOutcome::BadRules(e) => writeln!(f, "{name}: bad simulation rules: {e}")?,
            }
        }
        if !self.unaudited_missing.is_empty() {
            writeln!(
                f,
                "{} linked crates are not on the audit list (determinism.toml):",
                self.unaudited_missing.len()
            )?;
            for krate in &self.unaudited_missing {
                writeln!(f, "  {} {} ({})", krate.name, krate.version, krate.source)?;
            }
        }
        writeln!(
            f,
            "{scanned} local crates scanned ({findings} findings), {not_run} not run in \
             simulations; {} external crates audited, {} accepted unaudited, {} not on the list",
            self.audited,
            self.unaudited.len(),
            self.unaudited_missing.len()
        )?;
        if self.passed() {
            write!(f, "determinism check passed")
        } else {
            write!(
                f,
                "determinism check FAILED: use what the simulation controls (or mark the line \
                 `// simulation-safe: <reason>`), state a crate's rules in its \
                 [package.metadata.switchy.simulation], and audit new crates in determinism.toml"
            )
        }
    }
}
