//! `switchy_cargo`: check that everything a simulation links runs deterministically under it.
//! See the library documentation.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{path::PathBuf, process::ExitCode};

use clap::Parser;

/// Check that everything a simulation links runs deterministically under it.
#[derive(Debug, Parser)]
#[command(name = "switchy_cargo", version)]
struct Args {
    /// The simulation workspace's Cargo.toml (its determinism.toml sits beside it).
    #[arg(long, default_value = "Cargo.toml")]
    manifest_path: PathBuf,
    /// The simulation's package (more than one may be given).
    #[arg(long = "package", short = 'p', required = true)]
    packages: Vec<String>,
    /// Print audit entries (verdict `unaudited`, with this reason) for the linked crates the
    /// list does not cover, to accept them as they are, instead of failing on them.
    #[arg(long, value_name = "REASON")]
    baseline: Option<String>,
    /// Scan every external crate by the shared rules instead, as evidence for its audit: one
    /// line per crate with its finding count (and the first few with `--verbose`).
    #[arg(long)]
    scan_external: bool,
    /// With `--scan-external`, show each crate's first findings.
    #[arg(long)]
    verbose: bool,
    /// Check the `runs = false` claims instead, against this `llvm-cov export -summary-only`
    /// of the simulation's coverage-instrumented runs: fail if a file of such a crate ran.
    #[arg(long, value_name = "EXPORT")]
    coverage: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let mut passed = true;
    let mut baseline = std::collections::BTreeSet::new();
    if let Some(export) = &args.coverage {
        return check_claims(&args, export);
    }
    if args.scan_external {
        let mut seen = std::collections::BTreeSet::new();
        for package in &args.packages {
            let scanned = switchy_cargo::Check::with_workspace_config(&args.manifest_path, package)
                .and_then(|check| check.scan_external());
            match scanned {
                Ok(crates) => {
                    for (krate, findings) in crates {
                        if !seen.insert((krate.name.clone(), krate.version.clone())) {
                            continue;
                        }
                        println!("{} {} {}", findings.len(), krate.name, krate.version);
                        if args.verbose {
                            for finding in findings.iter().take(5) {
                                println!("    {finding}");
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{package}: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        return ExitCode::SUCCESS;
    }
    for package in &args.packages {
        let report = switchy_cargo::Check::with_workspace_config(&args.manifest_path, package)
            .and_then(|check| check.run());
        match report {
            Ok(report) => {
                if let Some(reason) = &args.baseline {
                    baseline.insert(report.baseline(reason));
                    continue;
                }
                println!("== {package}\n{report}");
                passed &= report.passed();
            }
            Err(e) => {
                eprintln!("{package}: {e}");
                passed = false;
            }
        }
    }
    if args.baseline.is_some() {
        // Entries shared by several simulations appear once.
        let mut entries = std::collections::BTreeSet::new();
        for text in baseline {
            for entry in text.split("\n[[audit]]").filter(|e| !e.trim().is_empty()) {
                entries.insert(entry.to_owned());
            }
        }
        for entry in entries {
            print!("\n[[audit]]{entry}");
        }
    }
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `--coverage`: check every package's `runs = false` claims against the export.
fn check_claims(args: &Args, export: &std::path::Path) -> ExitCode {
    let executed = match std::fs::read_to_string(export)
        .map_err(|e| e.to_string())
        .and_then(|json| switchy_cargo::coverage::Executed::from_llvm_cov_export(&json))
    {
        Ok(executed) => executed,
        Err(e) => {
            eprintln!("{}: {e}", export.display());
            return ExitCode::FAILURE;
        }
    };
    let mut passed = true;
    for package in &args.packages {
        match switchy_cargo::Check::with_workspace_config(&args.manifest_path, package)
            .and_then(|check| check.run_claims(&executed))
        {
            Ok(broken) => {
                for (krate, files) in &broken {
                    println!("{package}: {krate} is marked `runs = false`, but ran:");
                    for (file, lines) in files {
                        println!("  {} ({lines} lines)", file.display());
                    }
                }
                passed &= broken.is_empty();
            }
            Err(e) => {
                eprintln!("{package}: {e}");
                passed = false;
            }
        }
    }
    println!(
        "{} files in the profile; `runs = false` claims {}",
        executed.files(),
        if passed { "held" } else { "BROKEN: remove the crate's `runs = false` so the check scans it" }
    );
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
