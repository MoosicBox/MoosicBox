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
}

fn main() -> ExitCode {
    let args = Args::parse();
    let mut passed = true;
    let mut baseline = std::collections::BTreeSet::new();
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
