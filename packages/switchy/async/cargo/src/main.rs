//! A command-line tool reporting async code that `#[inject_yields]` does not cover.
//!
//! It scans a crate's `src` (see [`switchy_async_cargo::uncovered`]) and reports every
//! `async fn`, async method, `async` block and `async` closure that no `#[inject_yields]`
//! (on it or an enclosing item) or `inject_yields_mod!` covers.
//!
//! # Usage
//!
//! ```bash
//! # Check the crate in CARGO_MANIFEST_DIR
//! switchy_async_cargo
//!
//! # Check a specific crate, leaving out `#[cfg(test)]` code
//! switchy_async_cargo --root /path/to/crate --skip-tests
//! ```
//!
//! # Exit Codes
//!
//! * 0 - Nothing uncovered
//! * 1 - Uncovered async code (or files that could not be checked) found

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{env, path::PathBuf, process};

use clap::Parser;

/// Reports async code `#[inject_yields]` does not cover.
#[derive(Parser)]
#[command(
    author,
    version,
    about = "Reports async fns, blocks and closures not covered by #[inject_yields]"
)]
struct Cli {
    /// The crate directory whose `src` to scan (default: `CARGO_MANIFEST_DIR`).
    #[arg(long)]
    root: Option<PathBuf>,

    /// Leave out code inside `#[cfg(test)]` items.
    #[arg(long)]
    skip_tests: bool,
}

/// Scans the crate and prints what is uncovered.
///
/// # Panics
///
/// * If neither `--root` nor `CARGO_MANIFEST_DIR` is given
fn main() {
    pretty_env_logger::init();

    let cli = Cli::parse();
    let root = cli.root.unwrap_or_else(|| {
        PathBuf::from(
            env::var_os("CARGO_MANIFEST_DIR").expect("pass --root or set CARGO_MANIFEST_DIR"),
        )
    });

    let findings: Vec<_> = switchy_async_cargo::uncovered(&root)
        .into_iter()
        .filter(|finding| !(cli.skip_tests && finding.cfg_test))
        .collect();
    if !findings.is_empty() {
        println!(
            "{} warning{}:",
            findings.len(),
            if findings.len() == 1 { "" } else { "s" }
        );
        for finding in findings {
            println!("warning: {finding}");
        }
        process::exit(1);
    }
}
