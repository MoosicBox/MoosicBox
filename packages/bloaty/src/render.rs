//! Terminal and streaming report rendering.

use std::fmt::Write as _;

use anyhow::Result;
use bytesize::ByteSize;
use serde_json::json;

use crate::{AnalysisReport, ScenarioReport, ScenarioStatus};

/// Renders an analysis report for terminal users.
#[must_use]
pub fn text(report: &AnalysisReport) -> String {
    let mut output = String::new();
    writeln!(
        output,
        "Bloaty: {} / {} ({:?}, profile {})",
        report.package, report.target_name, report.target_kind, report.profile
    )
    .expect("writing to String cannot fail");
    render_scenario(&mut output, "baseline", &report.baseline);
    for scenario in &report.comparisons {
        render_scenario(&mut output, "compare", scenario);
    }
    output
}

fn render_scenario(output: &mut String, label: &str, report: &ScenarioReport) {
    match &report.outcome {
        ScenarioStatus::Success { measurement } => {
            let delta = measurement.delta_bytes.map_or_else(String::new, |delta| {
                let sign = if delta >= 0 { '+' } else { '-' };
                let percent = measurement
                    .delta_percent
                    .as_deref()
                    .map_or_else(String::new, |percent| format!(", {percent}%"));
                format!(" ({sign}{}{percent})", ByteSize(delta.unsigned_abs()))
            });
            writeln!(
                output,
                "  {label:<8} {:<20} {}{delta}",
                report.scenario.name,
                ByteSize(measurement.size_bytes)
            )
            .expect("writing to String cannot fail");
        }
        ScenarioStatus::Failed { error } => {
            writeln!(
                output,
                "  {label:<8} {:<20} FAILED: {error}",
                report.scenario.name
            )
            .expect("writing to String cannot fail");
        }
    }
}

/// Serializes a reconstructable JSONL stream consisting of metadata and complete scenarios.
///
/// # Errors
///
/// * JSON serialization fails
pub fn jsonl(report: &AnalysisReport) -> Result<String> {
    let mut output = String::new();
    writeln!(
        output,
        "{}",
        serde_json::to_string(&json!({
            "type": "analysis",
            "schema_version": report.schema_version,
            "started_at": report.started_at,
            "package": report.package,
            "target_name": report.target_name,
            "target_kind": report.target_kind,
            "profile": report.profile,
            "compilation_target": report.compilation_target,
            "environment": report.environment,
        }))?
    )?;
    writeln!(
        output,
        "{}",
        serde_json::to_string(&json!({"type": "baseline", "report": report.baseline}))?
    )?;
    for comparison in &report.comparisons {
        writeln!(
            output,
            "{}",
            serde_json::to_string(&json!({"type": "comparison", "report": comparison}))?
        )?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArtifactKind, BuildEnvironment, FeatureConfig, Measurement, Scenario, ScenarioStatus,
    };

    fn report() -> AnalysisReport {
        AnalysisReport {
            schema_version: 1,
            started_at: 1,
            package: "app".to_owned(),
            target_name: "app".to_owned(),
            target_kind: ArtifactKind::Binary,
            profile: "release".to_owned(),
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
                        size_bytes: 100,
                        delta_bytes: None,
                        delta_percent: None,
                        fresh: false,
                    },
                },
            },
            comparisons: vec![ScenarioReport {
                duration_ms: 0,
                scenario: Scenario {
                    name: "broken".to_owned(),
                    config: FeatureConfig::default(),
                },
                outcome: ScenarioStatus::Failed {
                    error: "build failed".to_owned(),
                },
            }],
        }
    }

    #[test]
    fn text_distinguishes_measurements_and_failures() {
        let output = text(&report());
        assert!(output.contains("baseline"));
        assert!(output.contains("100 B"));
        assert!(output.contains("FAILED: build failed"));
    }

    #[test]
    fn jsonl_contains_reconstructable_record_types() {
        let output = jsonl(&report()).unwrap();
        let records = output
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0]["type"], "analysis");
        assert_eq!(records[1]["type"], "baseline");
        assert_eq!(records[2]["type"], "comparison");
        assert_eq!(records[2]["report"]["status"], "failed");
    }
}
