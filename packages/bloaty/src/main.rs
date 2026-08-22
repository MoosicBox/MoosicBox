//! Binary size analysis CLI for Rust workspace packages.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeSet, fs};

use anyhow::{Context, Result, bail};
use bloaty::{
    AnalysisReport, ReportComparison, Scenario, ScenarioComparison, VarianceReport, analyze,
    characterize_variance, compare_reports, feature_scenario, parse_feature_config,
    parse_named_scenario, render, validate_scenarios, workspace, write_json, write_jsonl,
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
    #[arg(short, long, required_unless_present_any = ["compare_reports", "characterize_variance"])]
    package: Option<String>,

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

    /// Compare two previously generated JSON reports instead of building artifacts.
    #[arg(long, value_names = ["BASELINE_REPORT", "CANDIDATE_REPORT"], num_args = 2, conflicts_with_all = ["package", "target", "baseline", "feature", "scenario", "characterize_variance"])]
    compare_reports: Option<Vec<String>>,

    /// Characterize variance across two or more compatible JSON reports.
    #[arg(long, value_name = "REPORT", num_args = 2.., conflicts_with_all = ["package", "target", "baseline", "feature", "scenario", "compare_reports"])]
    characterize_variance: Option<Vec<String>>,

    /// Fail comparison when a measured scenario grows by more than this many bytes.
    #[arg(long, requires = "compare_reports")]
    max_increase_bytes: Option<u64>,

    /// Fail comparison when a measured scenario grows by more than this percentage.
    #[arg(long, requires = "compare_reports", allow_hyphen_values = true)]
    max_increase_percent: Option<f64>,
}

fn main() -> Result<()> {
    pretty_env_logger::init();
    let args = Args::parse();
    if let Some(paths) = &args.compare_reports {
        return compare_saved_reports(paths, args.max_increase_bytes, args.max_increase_percent);
    }
    if let Some(paths) = &args.characterize_variance {
        return characterize_saved_reports(paths);
    }
    let package = args
        .package
        .as_deref()
        .context("--package is required for analysis")?;
    let metadata = MetadataCommand::new().no_deps().exec()?;
    let target = workspace::resolve_target(&metadata, package, args.target.as_deref())?;

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

fn characterize_saved_reports(paths: &[String]) -> Result<()> {
    let reports = paths
        .iter()
        .map(|path| {
            let bytes = fs::read(path).with_context(|| format!("failed to read report {path}"))?;
            serde_json::from_slice::<AnalysisReport>(&bytes)
                .with_context(|| format!("failed to parse report {path}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let variance = characterize_variance(&reports).map_err(anyhow::Error::msg)?;
    print_variance(&variance);
    if !variance.incompatibilities.is_empty() {
        bail!("variance reports are incompatible");
    }
    Ok(())
}

fn print_variance(variance: &VarianceReport) {
    if !variance.incompatibilities.is_empty() {
        eprintln!("Variance reports are incompatible:");
        for incompatibility in &variance.incompatibilities {
            eprintln!("  - {incompatibility}");
        }
        return;
    }
    println!("Bloaty variance across {} reports", variance.report_count);
    for scenario in &variance.scenarios {
        println!(
            "  {:<20} {} samples, {}..{} bytes, spread {} bytes ({}%)",
            scenario.name,
            scenario.samples,
            scenario.minimum_size_bytes,
            scenario.maximum_size_bytes,
            scenario.spread_bytes,
            scenario.spread_percent.as_deref().unwrap_or("undefined")
        );
    }
}

fn compare_saved_reports(
    paths: &[String],
    max_increase_bytes: Option<u64>,
    max_increase_percent: Option<f64>,
) -> Result<()> {
    let [baseline_path, candidate_path] = paths else {
        bail!("--compare-reports requires baseline and candidate report paths");
    };
    let baseline: AnalysisReport = serde_json::from_slice(
        &fs::read(baseline_path)
            .with_context(|| format!("failed to read baseline report {baseline_path}"))?,
    )
    .with_context(|| format!("failed to parse baseline report {baseline_path}"))?;
    let candidate: AnalysisReport = serde_json::from_slice(
        &fs::read(candidate_path)
            .with_context(|| format!("failed to read candidate report {candidate_path}"))?,
    )
    .with_context(|| format!("failed to parse candidate report {candidate_path}"))?;
    let comparison = compare_reports(&baseline, &candidate);
    print_comparison(&comparison);
    if !comparison.is_compatible() {
        bail!("reports are incompatible");
    }
    enforce_thresholds(&comparison, max_increase_bytes, max_increase_percent)?;
    Ok(())
}

fn enforce_thresholds(
    comparison: &ReportComparison,
    max_increase_bytes: Option<u64>,
    max_increase_percent: Option<f64>,
) -> Result<()> {
    for scenario in &comparison.scenarios {
        if let ScenarioComparison::Compared {
            name,
            delta_bytes,
            delta_percent,
            ..
        } = scenario
        {
            if let Some(limit) = max_increase_bytes
                && *delta_bytes > i64::try_from(limit).unwrap_or(i64::MAX)
            {
                bail!(
                    "scenario '{name}' increased by {delta_bytes} bytes, exceeding {limit} bytes"
                );
            }
            if let Some(limit) = max_increase_percent
                && delta_percent
                    .as_deref()
                    .and_then(|percent| percent.parse::<f64>().ok())
                    .is_some_and(|percent| percent > limit)
            {
                bail!(
                    "scenario '{name}' increased by {}%, exceeding {limit}%",
                    delta_percent.as_deref().unwrap_or("undefined")
                );
            }
        }
    }
    Ok(())
}

fn print_comparison(comparison: &ReportComparison) {
    if !comparison.is_compatible() {
        eprintln!("Reports are incompatible:");
        for incompatibility in &comparison.incompatibilities {
            eprintln!("  - {incompatibility}");
        }
        return;
    }
    println!("Bloaty report comparison");
    for scenario in &comparison.scenarios {
        match scenario {
            ScenarioComparison::Compared {
                name,
                baseline_size_bytes,
                candidate_size_bytes,
                delta_bytes,
                delta_percent,
            } => println!(
                "  {name:<20} {baseline_size_bytes} -> {candidate_size_bytes} ({delta_bytes:+} bytes, {}%)",
                delta_percent.as_deref().unwrap_or("undefined")
            ),
            ScenarioComparison::Added { name } => println!("  {name:<20} ADDED"),
            ScenarioComparison::Removed { name } => println!("  {name:<20} REMOVED"),
            ScenarioComparison::Unavailable { name, .. } => {
                println!("  {name:<20} UNAVAILABLE");
            }
        }
    }
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
