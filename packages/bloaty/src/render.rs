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
