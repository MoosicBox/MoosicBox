//! CLI entrypoint for the `clippier-md` formatter.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use anyhow::Result;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "clippier-md")]
#[command(about = "Configurable markdown formatter for clippier")]
struct Cli {
    #[command(subcommand)]
    command: clippier_md::cli::Commands,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let exit_code = clippier_md::cli::run(&cli.command)?;
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}
