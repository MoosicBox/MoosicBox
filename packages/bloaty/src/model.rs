//! Canonical analysis request and report types.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A Cargo feature configuration for one build scenario.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureConfig {
    /// Whether Cargo default features are enabled.
    pub default_features: bool,
    /// Explicit package features enabled in addition to the default-feature policy.
    pub features: BTreeSet<String>,
}

/// A named build configuration measured against a baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    /// Stable, human-readable scenario name.
    pub name: String,
    /// Cargo feature configuration.
    pub config: FeatureConfig,
}

/// Supported final Cargo artifact kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    /// Executable binary.
    Binary,
    /// Dynamic system library.
    Cdylib,
    /// Rust dynamic library.
    Dylib,
    /// Static system library.
    Staticlib,
}

impl ArtifactKind {
    /// Returns the Cargo target-selection flag.
    #[must_use]
    pub const fn cargo_flag(self) -> &'static str {
        match self {
            Self::Binary => "--bin",
            Self::Cdylib | Self::Dylib | Self::Staticlib => "--lib",
        }
    }
}

/// An unambiguous Cargo package and final artifact target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSelection {
    /// Cargo package ID used to match compiler messages.
    pub package_id: cargo_metadata::PackageId,
    /// Cargo package name used in build commands and reports.
    pub package_name: String,
    /// Cargo target name.
    pub target_name: String,
    /// Final artifact kind.
    pub kind: ArtifactKind,
    /// Target features Cargo requires.
    pub required_features: Vec<String>,
    /// Features declared by the package.
    pub available_features: BTreeSet<String>,
}

/// Reproducibility information for an analysis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildEnvironment {
    /// Full Rust compiler version output.
    pub rustc: String,
    /// Cargo version output.
    pub cargo: String,
    /// Host operating system.
    pub host_os: String,
    /// Host architecture.
    pub host_arch: String,
    /// Git revision when available.
    pub git_revision: Option<String>,
    /// Whether the Git checkout had local changes when available.
    pub git_dirty: Option<bool>,
}

/// A measured final artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurement {
    /// Exact artifact path reported by Cargo.
    pub artifact_path: String,
    /// Artifact size in bytes.
    pub size_bytes: u64,
    /// Collected built-in and optional metric outcomes.
    #[serde(default)]
    pub metrics: Vec<crate::MetricReport>,
    /// Signed byte difference from the baseline, absent on the baseline itself.
    pub delta_bytes: Option<i64>,
    /// Percentage difference from the baseline, absent when undefined.
    pub delta_percent: Option<String>,
    /// Whether Cargo reused a fresh artifact.
    pub fresh: bool,
}

/// Structured scenario outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ScenarioStatus {
    /// The build succeeded and produced a measurement.
    Success { measurement: Measurement },
    /// The scenario build or artifact resolution failed.
    Failed { error: String },
    /// The scenario cannot run in the current environment.
    Unsupported { reason: String },
    /// The scenario was intentionally not run.
    Skipped { reason: String },
}

/// Report for one baseline or comparison scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioReport {
    /// Scenario definition.
    pub scenario: Scenario,
    /// Wall-clock build and measurement duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
    /// Structured outcome.
    #[serde(flatten)]
    pub outcome: ScenarioStatus,
}

impl ScenarioReport {
    pub(crate) fn from_build(
        scenario: Scenario,
        result: anyhow::Result<crate::build::BuiltArtifact>,
        baseline_size: Option<u64>,
        duration_ms: u64,
    ) -> Self {
        let outcome = match result {
            Ok(artifact) => {
                let delta_bytes =
                    baseline_size.map(|baseline| signed_delta(artifact.size, baseline));
                #[allow(clippy::cast_precision_loss)]
                let delta_percent =
                    baseline_size
                        .filter(|baseline| *baseline != 0)
                        .map(|baseline| {
                            format!(
                                "{:.4}",
                                (artifact.size as f64 - baseline as f64) / baseline as f64 * 100.0
                            )
                        });
                ScenarioStatus::Success {
                    measurement: Measurement {
                        artifact_path: artifact.path.to_string(),
                        size_bytes: artifact.size,
                        metrics: std::iter::once(crate::metrics::artifact_file_size(
                            &artifact.path,
                            artifact.size,
                        ))
                        .chain(crate::metrics::optional_capabilities())
                        .collect(),
                        delta_bytes,
                        delta_percent,
                        fresh: artifact.fresh,
                    },
                }
            }
            Err(error) => ScenarioStatus::Failed {
                error: format!("{error:#}"),
            },
        };
        Self {
            scenario,
            duration_ms,
            outcome,
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

/// Canonical versioned analysis report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Unix timestamp at analysis start.
    pub started_at: u64,
    /// Selected Cargo package.
    pub package: String,
    /// Selected Cargo target.
    pub target_name: String,
    /// Selected target kind.
    pub target_kind: ArtifactKind,
    /// Selected Cargo profile.
    pub profile: String,
    /// Optional compilation target triple.
    pub compilation_target: Option<String>,
    /// Build environment provenance.
    pub environment: BuildEnvironment,
    /// Explicit baseline result.
    pub baseline: ScenarioReport,
    /// Ordered comparison results.
    pub comparisons: Vec<ScenarioReport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_all_scenario_outcomes() {
        let statuses = [
            ScenarioStatus::Failed {
                error: "failed".to_owned(),
            },
            ScenarioStatus::Unsupported {
                reason: "unsupported".to_owned(),
            },
            ScenarioStatus::Skipped {
                reason: "skipped".to_owned(),
            },
        ];
        let serialized = statuses
            .iter()
            .map(|status| serde_json::to_value(status).unwrap()["status"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            serialized,
            ["failed", "unsupported", "skipped"].map(serde_json::Value::from)
        );
    }

    #[test]
    fn report_schema_round_trips() {
        let report = AnalysisReport {
            schema_version: 1,
            started_at: 0,
            package: "app".to_owned(),
            target_name: "app".to_owned(),
            target_kind: ArtifactKind::Binary,
            profile: "release".to_owned(),
            compilation_target: None,
            environment: BuildEnvironment {
                rustc: "rustc".to_owned(),
                cargo: "cargo".to_owned(),
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
                outcome: ScenarioStatus::Failed {
                    error: "failure".to_owned(),
                },
            },
            comparisons: Vec::new(),
        };
        let serialized = serde_json::to_vec(&report).unwrap();
        let restored: AnalysisReport = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored, report);
    }

    #[test]
    fn calculates_saturating_signed_deltas() {
        assert_eq!(signed_delta(12, 10), 2);
        assert_eq!(signed_delta(8, 10), -2);
        assert_eq!(signed_delta(u64::MAX, 0), i64::MAX);
    }
}
