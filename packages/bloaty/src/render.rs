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

/// Renders collapsible, deterministically sorted Markdown views of measurements.
#[must_use]
pub fn markdown(report: &AnalysisReport) -> String {
    let mut output = format!(
        "## Bloaty: {} / {}\n\nProfile: **{}**. Deltas compare features within this commit, not historical changes.\n\n",
        escape(&report.package),
        escape(&report.target_name),
        escape(&report.profile)
    );
    let mut failures = std::iter::once(&report.baseline)
        .chain(&report.comparisons)
        .filter(|row| !matches!(row.outcome, ScenarioStatus::Success { .. }))
        .collect::<Vec<_>>();
    failures.sort_by_key(|row| &row.scenario.name);
    if !failures.is_empty() {
        output.push_str("### Failed or unavailable measurements\n\n");
        for row in failures {
            writeln!(output, "- {}", escape(scenario_text("", row).trim()))
                .expect("writing to String cannot fail");
        }
        output.push('\n');
    }
    writeln!(
        output,
        "### Baseline\n\nDefaults: **{}**. Explicit features: **{}**.\n\n{}\n\nDuration: {} ms.\n",
        report.baseline.scenario.config.default_features,
        feature_names(&report.baseline),
        escape(scenario_text("baseline", &report.baseline).trim()),
        report.baseline.duration_ms
    )
    .expect("writing to String cannot fail");
    let mut rows = report
        .comparisons
        .iter()
        .filter_map(|row| {
            if let ScenarioStatus::Success { measurement } = &row.outcome {
                Some((row, measurement))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        output.push_str("No successful comparison measurements.\n\n");
    } else {
        for (index, title) in [
            "Largest size increase first",
            "Largest total binary size first",
            "Feature name A–Z",
            "Slowest builds first",
        ]
        .iter()
        .enumerate()
        {
            rows.sort_by(|(a, am), (b, bm)| {
                let order = match index {
                    0 => bm.delta_bytes.cmp(&am.delta_bytes),
                    1 => bm.size_bytes.cmp(&am.size_bytes),
                    2 => a.scenario.name.cmp(&b.scenario.name),
                    _ => b.duration_ms.cmp(&a.duration_ms),
                };
                order.then_with(|| a.scenario.name.cmp(&b.scenario.name))
            });
            writeln!(output, "<details{}>\n<summary>{title}</summary>\n\n| Scenario | Defaults | Explicit features | Total size | Size delta | Delta % | Duration (ms) |\n| --- | --- | --- | ---: | ---: | ---: | ---: |",
                if index == 0 { " open" } else { "" }
            ).expect("writing to String cannot fail");
            for (row, measurement) in &rows {
                writeln!(
                    output,
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    escape(&row.scenario.name),
                    row.scenario.config.default_features,
                    feature_names(row),
                    ByteSize(measurement.size_bytes),
                    measurement
                        .delta_bytes
                        .map_or_else(|| "—".to_owned(), signed_size),
                    escape(measurement.delta_percent.as_deref().unwrap_or("—")),
                    row.duration_ms
                )
                .expect("writing to String cannot fail");
            }
            output.push_str("\n</details>\n\n");
        }
    }
    writeln!(output, "Platform: {}/{}. Rust: {}. Commit: {}.\n\nExact final artifact sizes, not runtime memory or dependency attribution. Missing deltas are shown as —, not zero. Durations include cache reuse and are not clean-build benchmarks. Full reports are retained as artifacts.\n",
        escape(&report.environment.host_os), escape(&report.environment.host_arch),
        escape(report.environment.rustc.lines().next().unwrap_or("unknown")),
        escape(report.environment.git_revision.as_deref().unwrap_or("unknown"))
    ).expect("writing to String cannot fail");
    output
}

/// Formats a signed byte delta using human-readable binary units.
#[must_use]
pub fn signed_size(value: i64) -> String {
    let sign = if value < 0 { '-' } else { '+' };
    format!("{sign}{}", ByteSize(value.unsigned_abs()))
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace('`', "&#96;")
        .replace(['\r', '\n'], " ")
}

fn feature_names(row: &ScenarioReport) -> String {
    if row.scenario.config.features.is_empty() {
        "none".to_owned()
    } else {
        escape(
            &row.scenario
                .config
                .features
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

/// Renders one completed scenario for live progress output.
#[must_use]
pub fn scenario_text(label: &str, report: &ScenarioReport) -> String {
    let mut output = String::new();
    render_scenario(&mut output, label, report);
    output
}

fn render_scenario(output: &mut String, label: &str, report: &ScenarioReport) {
    match &report.outcome {
        ScenarioStatus::Success { measurement } => {
            let delta = measurement.delta_bytes.map_or_else(String::new, |delta| {
                let percent = measurement
                    .delta_percent
                    .as_deref()
                    .map_or_else(String::new, |percent| format!(", {percent}%"));
                format!(" ({}{percent})", signed_size(delta))
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
        ScenarioStatus::Unsupported { reason } => {
            writeln!(
                output,
                "  {label:<8} {:<20} UNSUPPORTED: {reason}",
                report.scenario.name
            )
            .expect("writing to String cannot fail");
        }
        ScenarioStatus::Skipped { reason } => {
            writeln!(
                output,
                "  {label:<8} {:<20} SKIPPED: {reason}",
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
                        metrics: Vec::new(),
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
    fn markdown_includes_features_failures_and_escaped_names() {
        let mut report = report();
        report.package = "<app>|name".to_owned();
        let output = markdown(&report);
        assert!(output.contains("&lt;app&gt;&#124;name"));
        assert!(output.contains("Defaults: **false**. Explicit features: **none**"));
        assert!(output.contains("FAILED: build failed"));
        assert!(output.contains("100 B"));
        assert!(output.contains("not historical changes"));
    }

    #[test]
    fn markdown_sorts_numeric_views_without_mutating_reports() {
        let mut report = report();
        for (name, size, delta, duration) in [
            ("z", 200, Some(100), 2),
            ("a", 90, Some(-10), 300),
            ("b", 200, Some(100), 20),
            ("unknown", 500, None, 1),
        ] {
            let mut row = report.baseline.clone();
            row.scenario.name = name.to_owned();
            row.duration_ms = duration;
            if let ScenarioStatus::Success { measurement } = &mut row.outcome {
                measurement.size_bytes = size;
                measurement.delta_bytes = delta;
            }
            report.comparisons.push(row);
        }
        let original = report.clone();
        let output = markdown(&report);
        let views = output
            .split("</summary>")
            .skip(1)
            .map(|part| {
                part.split("</details>")
                    .next()
                    .unwrap()
                    .lines()
                    .filter(|line| line.starts_with("| "))
                    .skip(2)
                    .map(|line| line.split('|').nth(1).unwrap().trim())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            views,
            vec![
                vec!["b", "z", "a", "unknown"],
                vec!["unknown", "b", "z", "a"],
                vec!["a", "b", "unknown", "z"],
                vec!["a", "b", "z", "unknown"],
            ]
        );
        assert_eq!(report, original);
        assert_eq!(output.matches("<details open>").count(), 1);
        assert_eq!(output.matches("### Baseline").count(), 1);
        assert!(output.find("FAILED").unwrap() < output.find("<details").unwrap());
        assert!(output.contains("| unknown | false | none | 500 B | — | — | 1 |"));
    }

    #[test]
    fn signed_sizes_use_binary_units_and_preserve_signs() {
        assert_eq!(signed_size(0), "+0 B");
        assert_eq!(signed_size(1024), "+1.0 KiB");
        assert_eq!(signed_size(-1_048_576), "-1.0 MiB");
        assert_eq!(signed_size(1_073_741_824), "+1.0 GiB");
        assert!(signed_size(i64::MIN).starts_with('-'));
    }

    #[test]
    fn markdown_handles_no_successful_comparisons() {
        let output = markdown(&report());
        assert!(output.contains("No successful comparison measurements"));
        assert!(!output.contains("<details"));
    }

    #[test]
    fn live_scenarios_match_the_final_report() {
        let report = report();
        let progress = scenario_text("baseline", &report.baseline)
            + &scenario_text("compare", &report.comparisons[0]);
        assert!(text(&report).ends_with(&progress));
        assert!(progress.contains("100 B"));
        assert!(progress.contains("FAILED: build failed"));
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
