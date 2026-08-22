//! Compatibility-aware comparison of persisted Bloaty reports.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{AnalysisReport, ScenarioReport, ScenarioStatus};

/// Compatibility dimensions that must match before artifact sizes are comparable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonKey {
    /// Report schema version.
    pub schema_version: u32,
    /// Cargo package name.
    pub package: String,
    /// Cargo target name.
    pub target_name: String,
    /// Cargo target kind.
    pub target_kind: crate::ArtifactKind,
    /// Cargo profile.
    pub profile: String,
    /// Optional Rust compilation target.
    pub compilation_target: Option<String>,
    /// Complete Rust compiler identity.
    pub rustc: String,
    /// Host operating system.
    pub host_os: String,
    /// Host architecture.
    pub host_arch: String,
    /// Metric being compared.
    pub metric: String,
}

impl From<&AnalysisReport> for ComparisonKey {
    fn from(report: &AnalysisReport) -> Self {
        Self {
            schema_version: report.schema_version,
            package: report.package.clone(),
            target_name: report.target_name.clone(),
            target_kind: report.target_kind,
            profile: report.profile.clone(),
            compilation_target: report.compilation_target.clone(),
            rustc: report.environment.rustc.clone(),
            host_os: report.environment.host_os.clone(),
            host_arch: report.environment.host_arch.clone(),
            metric: "artifact-file-size-bytes".to_owned(),
        }
    }
}

/// Outcome for one scenario across two compatible reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ScenarioComparison {
    /// Both reports measured the scenario.
    Compared {
        /// Scenario name.
        name: String,
        /// Baseline report size.
        baseline_size_bytes: u64,
        /// Candidate report size.
        candidate_size_bytes: u64,
        /// Signed candidate-minus-baseline byte difference.
        delta_bytes: i64,
        /// Percentage difference, absent when the baseline is zero.
        delta_percent: Option<String>,
    },
    /// The scenario only exists in the candidate report.
    Added { name: String },
    /// The scenario only exists in the baseline report.
    Removed { name: String },
    /// At least one report did not successfully measure the scenario.
    Unavailable {
        name: String,
        baseline_error: Option<String>,
        candidate_error: Option<String>,
    },
}

/// Observed spread for one scenario across compatible reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioVariance {
    /// Scenario name.
    pub name: String,
    /// Number of successful measurements.
    pub samples: usize,
    /// Smallest observed artifact size.
    pub minimum_size_bytes: u64,
    /// Largest observed artifact size.
    pub maximum_size_bytes: u64,
    /// Absolute observed spread.
    pub spread_bytes: u64,
    /// Spread as a percentage of the minimum, absent when the minimum is zero.
    pub spread_percent: Option<String>,
}

/// Compatibility-aware variance characterization for repeated reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarianceReport {
    /// Compatibility key shared by all samples.
    pub key: ComparisonKey,
    /// Number of reports examined.
    pub report_count: usize,
    /// Compatibility errors. Scenario results are empty when this is non-empty.
    pub incompatibilities: Vec<String>,
    /// Per-scenario observed spreads.
    pub scenarios: Vec<ScenarioVariance>,
}

/// Characterizes observed measurement spread across repeated compatible reports.
///
/// # Errors
///
/// * Fewer than two reports are provided
pub fn characterize_variance(reports: &[AnalysisReport]) -> Result<VarianceReport, String> {
    let Some(first) = reports.first() else {
        return Err("at least two reports are required".to_owned());
    };
    if reports.len() < 2 {
        return Err("at least two reports are required".to_owned());
    }
    let key = ComparisonKey::from(first);
    let incompatibilities = reports
        .iter()
        .skip(1)
        .enumerate()
        .flat_map(|(index, report)| {
            incompatibilities(&key, &ComparisonKey::from(report))
                .into_iter()
                .map(move |difference| format!("report {}: {difference}", index + 2))
        })
        .collect::<Vec<_>>();
    let scenarios = if incompatibilities.is_empty() {
        scenario_variance(reports)
    } else {
        Vec::new()
    };
    Ok(VarianceReport {
        key,
        report_count: reports.len(),
        incompatibilities,
        scenarios,
    })
}

fn scenario_variance(reports: &[AnalysisReport]) -> Vec<ScenarioVariance> {
    let names = reports
        .iter()
        .flat_map(|report| scenario_map(report).into_keys())
        .collect::<BTreeSet<_>>();
    names
        .into_iter()
        .filter_map(|name| {
            let sizes = reports
                .iter()
                .filter_map(|report| scenario_map(report).get(&name).copied())
                .filter_map(|report| match &report.outcome {
                    ScenarioStatus::Success { measurement } => Some(measurement.size_bytes),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let minimum = sizes.iter().copied().min()?;
            let maximum = sizes.iter().copied().max()?;
            let spread = maximum - minimum;
            #[allow(clippy::cast_precision_loss)]
            let spread_percent =
                (minimum != 0).then(|| format!("{:.4}", spread as f64 / minimum as f64 * 100.0));
            Some(ScenarioVariance {
                name,
                samples: sizes.len(),
                minimum_size_bytes: minimum,
                maximum_size_bytes: maximum,
                spread_bytes: spread,
                spread_percent,
            })
        })
        .collect()
}

/// Complete compatibility-aware report comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportComparison {
    /// Baseline report compatibility key.
    pub baseline_key: ComparisonKey,
    /// Candidate report compatibility key.
    pub candidate_key: ComparisonKey,
    /// Differences that make these reports incompatible.
    pub incompatibilities: Vec<String>,
    /// Scenario outcomes, empty when reports are incompatible.
    pub scenarios: Vec<ScenarioComparison>,
}

impl ReportComparison {
    /// Returns whether all required comparison dimensions match.
    #[must_use]
    pub const fn is_compatible(&self) -> bool {
        self.incompatibilities.is_empty()
    }
}

/// Compares two reports without silently comparing incompatible build environments.
#[must_use]
pub fn compare_reports(baseline: &AnalysisReport, candidate: &AnalysisReport) -> ReportComparison {
    let baseline_key = ComparisonKey::from(baseline);
    let candidate_key = ComparisonKey::from(candidate);
    let incompatibilities = incompatibilities(&baseline_key, &candidate_key);
    let scenarios = if incompatibilities.is_empty() {
        compare_scenarios(baseline, candidate)
    } else {
        Vec::new()
    };
    ReportComparison {
        baseline_key,
        candidate_key,
        incompatibilities,
        scenarios,
    }
}

fn incompatibilities(baseline: &ComparisonKey, candidate: &ComparisonKey) -> Vec<String> {
    let mut differences = Vec::new();
    macro_rules! compare {
        ($field:ident) => {
            if baseline.$field != candidate.$field {
                differences.push(format!(
                    "{} differs: baseline={:?}, candidate={:?}",
                    stringify!($field),
                    baseline.$field,
                    candidate.$field
                ));
            }
        };
    }
    compare!(schema_version);
    compare!(package);
    compare!(target_name);
    compare!(target_kind);
    compare!(profile);
    compare!(compilation_target);
    compare!(rustc);
    compare!(host_os);
    compare!(host_arch);
    compare!(metric);
    differences
}

fn compare_scenarios(
    baseline: &AnalysisReport,
    candidate: &AnalysisReport,
) -> Vec<ScenarioComparison> {
    let baseline = scenario_map(baseline);
    let candidate = scenario_map(candidate);
    let names = baseline
        .keys()
        .chain(candidate.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    names
        .into_iter()
        .map(|name| match (baseline.get(&name), candidate.get(&name)) {
            (Some(baseline), Some(candidate)) => compare_scenario(&name, baseline, candidate),
            (None, Some(_)) => ScenarioComparison::Added { name },
            (Some(_), None) => ScenarioComparison::Removed { name },
            (None, None) => unreachable!("name came from one of the scenario maps"),
        })
        .collect()
}

fn scenario_map(report: &AnalysisReport) -> BTreeMap<String, &ScenarioReport> {
    std::iter::once(&report.baseline)
        .chain(&report.comparisons)
        .map(|report| (report.scenario.name.clone(), report))
        .collect()
}

fn compare_scenario(
    name: &str,
    baseline: &ScenarioReport,
    candidate: &ScenarioReport,
) -> ScenarioComparison {
    match (&baseline.outcome, &candidate.outcome) {
        (
            ScenarioStatus::Success {
                measurement: baseline,
            },
            ScenarioStatus::Success {
                measurement: candidate,
            },
        ) => {
            let delta_bytes = signed_delta(candidate.size_bytes, baseline.size_bytes);
            #[allow(clippy::cast_precision_loss)]
            let delta_percent = (baseline.size_bytes != 0).then(|| {
                format!(
                    "{:.4}",
                    (candidate.size_bytes as f64 - baseline.size_bytes as f64)
                        / baseline.size_bytes as f64
                        * 100.0
                )
            });
            ScenarioComparison::Compared {
                name: name.to_owned(),
                baseline_size_bytes: baseline.size_bytes,
                candidate_size_bytes: candidate.size_bytes,
                delta_bytes,
                delta_percent,
            }
        }
        (baseline, candidate) => ScenarioComparison::Unavailable {
            name: name.to_owned(),
            baseline_error: error(baseline),
            candidate_error: error(candidate),
        },
    }
}

fn error(status: &ScenarioStatus) -> Option<String> {
    match status {
        ScenarioStatus::Success { .. } => None,
        ScenarioStatus::Failed { error } => Some(error.clone()),
        ScenarioStatus::Unsupported { reason } | ScenarioStatus::Skipped { reason } => {
            Some(reason.clone())
        }
    }
}

fn signed_delta(value: u64, baseline: u64) -> i64 {
    if value >= baseline {
        i64::try_from(value - baseline).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(baseline - value).unwrap_or(i64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ArtifactKind, BuildEnvironment, FeatureConfig, Measurement, Scenario, ScenarioReport,
        ScenarioStatus,
    };

    use super::*;

    fn report(profile: &str, size: u64) -> AnalysisReport {
        AnalysisReport {
            schema_version: 1,
            started_at: 0,
            package: "app".to_owned(),
            target_name: "app".to_owned(),
            target_kind: ArtifactKind::Binary,
            profile: profile.to_owned(),
            compilation_target: None,
            environment: BuildEnvironment {
                rustc: "rustc 1".to_owned(),
                cargo: "cargo 1".to_owned(),
                host_os: "linux".to_owned(),
                host_arch: "x86_64".to_owned(),
                git_revision: None,
                git_dirty: None,
            },
            baseline: ScenarioReport {
                duration_ms: 0,
                scenario: Scenario {
                    name: "baseline".to_owned(),
                    config: FeatureConfig::default(),
                },
                outcome: ScenarioStatus::Success {
                    measurement: Measurement {
                        artifact_path: "app".to_owned(),
                        size_bytes: size,
                        metrics: Vec::new(),
                        delta_bytes: None,
                        delta_percent: None,
                        fresh: false,
                    },
                },
            },
            comparisons: Vec::new(),
        }
    }

    #[test]
    fn characterizes_compatible_variance() {
        let variance = characterize_variance(&[
            report("release", 100),
            report("release", 101),
            report("release", 99),
        ])
        .unwrap();
        assert!(variance.incompatibilities.is_empty());
        assert_eq!(variance.scenarios[0].samples, 3);
        assert_eq!(variance.scenarios[0].spread_bytes, 2);
        assert_eq!(
            variance.scenarios[0].spread_percent.as_deref(),
            Some("2.0202")
        );
    }

    #[test]
    fn rejects_incompatible_variance_samples() {
        let variance =
            characterize_variance(&[report("release", 100), report("dev", 100)]).unwrap();
        assert!(!variance.incompatibilities.is_empty());
        assert!(variance.scenarios.is_empty());
    }

    #[test]
    fn compares_compatible_reports() {
        let comparison = compare_reports(&report("release", 100), &report("release", 125));
        assert!(comparison.is_compatible());
        assert!(matches!(
            comparison.scenarios.as_slice(),
            [ScenarioComparison::Compared {
                delta_bytes: 25,
                ..
            }]
        ));
    }

    #[test]
    fn classifies_added_removed_and_failed_scenarios() {
        let mut baseline = report("release", 100);
        baseline.comparisons.push(ScenarioReport {
            duration_ms: 0,
            scenario: Scenario {
                name: "removed".to_owned(),
                config: FeatureConfig::default(),
            },
            outcome: ScenarioStatus::Success {
                measurement: Measurement {
                    artifact_path: "removed".to_owned(),
                    size_bytes: 10,
                    metrics: Vec::new(),
                    delta_bytes: None,
                    delta_percent: None,
                    fresh: false,
                },
            },
        });
        baseline.comparisons.push(ScenarioReport {
            duration_ms: 0,
            scenario: Scenario {
                name: "failed".to_owned(),
                config: FeatureConfig::default(),
            },
            outcome: ScenarioStatus::Failed {
                error: "baseline failure".to_owned(),
            },
        });
        let mut candidate = report("release", 100);
        candidate.comparisons.push(ScenarioReport {
            duration_ms: 0,
            scenario: Scenario {
                name: "added".to_owned(),
                config: FeatureConfig::default(),
            },
            outcome: ScenarioStatus::Success {
                measurement: Measurement {
                    artifact_path: "added".to_owned(),
                    size_bytes: 10,
                    metrics: Vec::new(),
                    delta_bytes: None,
                    delta_percent: None,
                    fresh: false,
                },
            },
        });
        candidate.comparisons.push(ScenarioReport {
            duration_ms: 0,
            scenario: Scenario {
                name: "failed".to_owned(),
                config: FeatureConfig::default(),
            },
            outcome: ScenarioStatus::Success {
                measurement: Measurement {
                    artifact_path: "failed".to_owned(),
                    size_bytes: 10,
                    metrics: Vec::new(),
                    delta_bytes: None,
                    delta_percent: None,
                    fresh: false,
                },
            },
        });

        let comparison = compare_reports(&baseline, &candidate);
        assert!(comparison.is_compatible());
        assert!(comparison.scenarios.iter().any(
            |scenario| matches!(scenario, ScenarioComparison::Added { name } if name == "added")
        ));
        assert!(comparison.scenarios.iter().any(
            |scenario| matches!(scenario, ScenarioComparison::Removed { name } if name == "removed")
        ));
        assert!(comparison.scenarios.iter().any(|scenario| matches!(
            scenario,
            ScenarioComparison::Unavailable {
                name,
                baseline_error: Some(_),
                candidate_error: None,
            } if name == "failed"
        )));
    }

    #[test]
    fn rejects_incompatible_profiles() {
        let comparison = compare_reports(&report("release", 100), &report("dev", 125));
        assert!(!comparison.is_compatible());
        assert!(comparison.incompatibilities[0].contains("profile differs"));
        assert!(comparison.scenarios.is_empty());
    }
}
