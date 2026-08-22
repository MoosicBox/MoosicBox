//! Typed analysis scenarios and reports for the Bloaty CLI.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod build;
pub mod compare;
pub mod metrics;
pub mod model;
pub mod render;
pub mod workspace;

use std::{
    collections::BTreeSet,
    fs,
    process::Command,
    time::{Instant, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use cargo_metadata::Metadata;

pub use compare::{
    ComparisonKey, ReportComparison, ScenarioComparison, ScenarioVariance, VarianceReport,
    characterize_variance, compare_reports,
};
pub use metrics::{
    MetricCapability, MetricKind, MetricOutcome, MetricRecord, MetricReport, run_external_collector,
};
pub use model::{
    AnalysisReport, ArtifactKind, BuildEnvironment, FeatureConfig, Measurement, Scenario,
    ScenarioReport, ScenarioStatus, TargetSelection,
};

/// Parses a feature configuration.
///
/// `default` enables default features, `none` disables them, and any other comma-separated
/// entries are explicit package features. `default` can be combined with explicit features.
///
/// # Errors
///
/// * The specification is empty or combines `none` with another value
pub fn parse_feature_config(value: &str) -> Result<FeatureConfig> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        bail!("feature configuration cannot be empty");
    }
    if values.contains(&"none") && values.len() != 1 {
        bail!("'none' cannot be combined with other features");
    }

    let default_features = values.contains(&"default");
    let features = values
        .into_iter()
        .filter(|value| *value != "default" && *value != "none")
        .map(ToOwned::to_owned)
        .collect();
    Ok(FeatureConfig {
        default_features,
        features,
    })
}

/// Creates an individual-feature scenario by adding a feature to a baseline configuration.
#[must_use]
pub fn feature_scenario(baseline: &FeatureConfig, feature: &str) -> Scenario {
    let mut config = baseline.clone();
    config.features.insert(feature.to_owned());
    Scenario {
        name: feature.to_owned(),
        config,
    }
}

/// Parses a named scenario in `NAME=FEATURE_SPECIFICATION` form.
///
/// # Errors
///
/// * The name or feature specification is missing or invalid
pub fn parse_named_scenario(value: &str) -> Result<Scenario> {
    let (name, config) = value
        .split_once('=')
        .context("scenario must use NAME=FEATURE_SPECIFICATION syntax")?;
    let name = name.trim();
    if name.is_empty() {
        bail!("scenario name cannot be empty");
    }
    Ok(Scenario {
        name: name.to_owned(),
        config: parse_feature_config(config)?,
    })
}

/// Validates scenarios against package features.
///
/// Cargo remains authoritative for target `required-features` because aggregate features can
/// activate required features transitively.
///
/// # Errors
///
/// * A scenario names an unknown package feature
pub fn validate_scenarios(available: &BTreeSet<String>, scenarios: &[Scenario]) -> Result<()> {
    for scenario in scenarios {
        for feature in &scenario.config.features {
            if !available.contains(feature) {
                bail!(
                    "scenario '{}' enables unknown feature '{feature}'",
                    scenario.name
                );
            }
        }
    }
    Ok(())
}

/// Runs the baseline and comparison scenarios and returns a canonical report.
///
/// # Errors
///
/// * Build environment discovery or report setup fails
pub fn analyze(
    metadata: &Metadata,
    target: &TargetSelection,
    profile: &str,
    compilation_target: Option<&str>,
    baseline: Scenario,
    comparisons: Vec<Scenario>,
) -> Result<AnalysisReport> {
    let started_at = switchy_time::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    let environment = environment(metadata);

    let baseline_started = Instant::now();
    let baseline_build =
        build::build_scenario(metadata, target, profile, compilation_target, &baseline);
    let baseline_duration_ms =
        u64::try_from(baseline_started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let baseline_size = baseline_build.as_ref().ok().map(|artifact| artifact.size);
    let baseline_report =
        ScenarioReport::from_build(baseline, baseline_build, None, baseline_duration_ms);

    let comparison_reports = comparisons
        .into_iter()
        .map(|scenario| {
            let scenario_started = Instant::now();
            let result =
                build::build_scenario(metadata, target, profile, compilation_target, &scenario);
            let duration_ms =
                u64::try_from(scenario_started.elapsed().as_millis()).unwrap_or(u64::MAX);
            ScenarioReport::from_build(scenario, result, baseline_size, duration_ms)
        })
        .collect();

    Ok(AnalysisReport {
        schema_version: 1,
        started_at,
        package: target.package_name.clone(),
        target_name: target.target_name.clone(),
        target_kind: target.kind,
        profile: profile.to_owned(),
        compilation_target: compilation_target.map(ToOwned::to_owned),
        environment,
        baseline: baseline_report,
        comparisons: comparison_reports,
    })
}

fn environment(metadata: &Metadata) -> BuildEnvironment {
    fn output(command: &mut Command) -> Option<String> {
        let output = command.output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    let workspace = metadata.workspace_root.as_std_path();
    let rustc = output(Command::new("rustc").arg("-Vv")).unwrap_or_default();
    let cargo = output(Command::new("cargo").arg("-V")).unwrap_or_default();
    let git_revision = output(
        Command::new("git")
            .current_dir(workspace)
            .args(["rev-parse", "HEAD"]),
    );
    let git_dirty = Command::new("git")
        .current_dir(workspace)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| !output.stdout.is_empty());

    BuildEnvironment {
        rustc,
        cargo,
        host_os: std::env::consts::OS.to_owned(),
        host_arch: std::env::consts::ARCH.to_owned(),
        git_revision,
        git_dirty,
    }
}

/// Writes a report as pretty JSON.
///
/// # Errors
///
/// * Serialization or writing fails
pub fn write_json(path: &str, report: &AnalysisReport) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(report)?)
        .with_context(|| format!("failed to write JSON report to {path}"))
}

/// Writes a report as reconstructable JSONL records.
///
/// # Errors
///
/// * Serialization or writing fails
pub fn write_jsonl(path: &str, report: &AnalysisReport) -> Result<()> {
    let records = render::jsonl(report)?;
    fs::write(path, records).with_context(|| format!("failed to write JSONL report to {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_feature_configurations() {
        assert_eq!(
            parse_feature_config("default,qobuz").unwrap(),
            FeatureConfig {
                default_features: true,
                features: BTreeSet::from(["qobuz".to_owned()]),
            }
        );
        assert_eq!(
            parse_feature_config("none").unwrap(),
            FeatureConfig::default()
        );
        assert!(parse_feature_config("none,qobuz").is_err());
    }

    #[test]
    fn parses_named_combinations() {
        let scenario = parse_named_scenario("sources=qobuz,tidal").unwrap();
        assert_eq!(scenario.name, "sources");
        assert_eq!(scenario.config.features.len(), 2);
        assert!(!scenario.config.default_features);
    }

    #[test]
    fn parses_multiple_individual_features_as_deterministic_scenarios() {
        let baseline = FeatureConfig::default();
        let scenarios = ["tidal", "qobuz"]
            .into_iter()
            .map(|feature| feature_scenario(&baseline, feature))
            .collect::<Vec<_>>();
        assert_eq!(scenarios[0].name, "tidal");
        assert_eq!(scenarios[1].name, "qobuz");
        assert_eq!(
            scenarios[0].config.features,
            BTreeSet::from(["tidal".to_owned()])
        );
    }

    #[test]
    fn explicit_scenarios_are_bounded_to_user_input() {
        let scenarios = ["pair=qobuz,tidal", "all=all-sources"]
            .into_iter()
            .map(parse_named_scenario)
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(scenarios.len(), 2);
        assert_eq!(scenarios[0].config.features.len(), 2);
        assert_eq!(scenarios[1].config.features.len(), 1);
    }

    #[test]
    fn validates_all_explicit_feature_scenarios() {
        let scenarios = [Scenario {
            name: "valid".to_owned(),
            config: FeatureConfig {
                default_features: false,
                features: BTreeSet::from(["known".to_owned()]),
            },
        }];
        validate_scenarios(&BTreeSet::from(["known".to_owned()]), &scenarios).unwrap();
        assert!(
            validate_scenarios(&BTreeSet::new(), &scenarios)
                .unwrap_err()
                .to_string()
                .contains("unknown feature")
        );
    }

    #[test]
    fn rejects_unknown_features() {
        let scenarios = [Scenario {
            name: "unknown".to_owned(),
            config: FeatureConfig {
                default_features: false,
                features: BTreeSet::from(["missing".to_owned()]),
            },
        }];
        let error =
            validate_scenarios(&BTreeSet::from(["known".to_owned()]), &scenarios).unwrap_err();
        assert!(error.to_string().contains("unknown feature 'missing'"));
    }

    #[test]
    fn feature_scenarios_extend_the_baseline() {
        let baseline = parse_feature_config("default,qobuz").unwrap();
        let scenario = feature_scenario(&baseline, "tidal");
        assert!(scenario.config.default_features);
        assert_eq!(scenario.config.features.len(), 2);
    }
}
