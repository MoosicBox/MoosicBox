//! Binary size analysis CLI for Rust workspace packages.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeSet, fs};

use anyhow::{Context, Result, bail};
use bloaty::{
    Scenario, analyze, feature_scenario, parse_feature_config, parse_named_scenario, render,
    validate_scenarios, workspace, write_json, write_jsonl,
};
use cargo_metadata::MetadataCommand;
use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Jsonl,
    All,
}

/// Command-line arguments for final-artifact feature analysis.
#[derive(Parser)]
#[command(
    author,
    version,
    about = "Measure final Cargo artifact size across feature scenarios",
    after_help = "FEATURE SPECIFICATIONS:\n  none                 Disable default features\n  default              Enable default features\n  qobuz,tidal          Disable defaults and enable a combination\n  default,qobuz        Enable defaults plus an explicit feature\n\nEXAMPLES:\n  bloaty -p app --target app --baseline none --feature qobuz\n  bloaty -p app --target app --baseline default --scenario sources=qobuz,tidal"
)]
struct Args {
    /// Workspace package to analyze.
    #[arg(short, long)]
    package: String,

    /// Final artifact target name. May be omitted when the package has one supported target.
    #[arg(long)]
    target: Option<String>,

    /// Cargo profile (for example dev, release, debug-release, or small).
    #[arg(long, default_value = "release")]
    profile: String,

    /// Optional Rust compilation target triple.
    #[arg(long)]
    compilation_target: Option<String>,

    /// Baseline feature specification.
    #[arg(long, default_value = "none")]
    baseline: String,

    /// Compare an individual feature added to the baseline. Repeatable or comma-separated.
    #[arg(long, value_delimiter = ',')]
    feature: Vec<String>,

    /// Named comparison in `NAME=FEATURE_SPECIFICATION` form. Repeatable.
    #[arg(long)]
    scenario: Vec<String>,

    /// Output format. Text prints to stdout; file formats require --report-file.
    #[arg(long, value_enum, default_value = "text")]
    output_format: OutputFormat,

    /// Base report path without an extension.
    #[arg(long)]
    report_file: Option<String>,
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    let args = Args::parse();
    let metadata = MetadataCommand::new().no_deps().exec()?;
    let target = workspace::resolve_target(&metadata, &args.package, args.target.as_deref())?;

    let baseline = Scenario {
        name: "baseline".to_owned(),
        config: parse_feature_config(&args.baseline).context("invalid baseline")?,
    };
    let mut comparisons = args
        .feature
        .iter()
        .map(|feature| feature_scenario(&baseline.config, feature))
        .collect::<Vec<_>>();
    comparisons.extend(
        args.scenario
            .iter()
            .map(|scenario| parse_named_scenario(scenario))
            .collect::<Result<Vec<_>>>()?,
    );
    if comparisons.is_empty() {
        bail!("provide at least one --feature or --scenario comparison");
    }
    ensure_unique_names(&comparisons)?;

    let mut all_scenarios = Vec::with_capacity(comparisons.len() + 1);
    all_scenarios.push(baseline.clone());
    all_scenarios.extend(comparisons.iter().cloned());
    validate_scenarios(&target.available_features, &all_scenarios)?;

    let report = analyze(
        &metadata,
        &target,
        &args.profile,
        args.compilation_target.as_deref(),
        baseline,
        comparisons,
    )?;
    let text = render::text(&report);
    match args.output_format {
        OutputFormat::Text => print!("{text}"),
        OutputFormat::Json => write_json(&report_path(&args, "json")?, &report)?,
        OutputFormat::Jsonl => write_jsonl(&report_path(&args, "jsonl")?, &report)?,
        OutputFormat::All => {
            print!("{text}");
            let base = args
                .report_file
                .as_deref()
                .context("--report-file is required for --output-format all")?;
            fs::write(format!("{base}.txt"), text)?;
            write_json(&format!("{base}.json"), &report)?;
            write_jsonl(&format!("{base}.jsonl"), &report)?;
        }
    }
    Ok(())
}

fn ensure_unique_names(scenarios: &[Scenario]) -> Result<()> {
    let mut names = BTreeSet::new();
    for scenario in scenarios {
        if !names.insert(&scenario.name) {
            bail!("duplicate comparison scenario name '{}'", scenario.name);
        }
    }
    Ok(())
}

fn report_path(args: &Args, extension: &str) -> Result<String> {
    let base = args
        .report_file
        .as_deref()
        .context("--report-file is required for file output")?;
    Ok(format!("{base}.{extension}"))
}
