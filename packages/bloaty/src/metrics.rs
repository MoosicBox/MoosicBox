//! Artifact metric collection with explicit capability and failure reporting.

use std::{fs, process::Command};

use cargo_metadata::camino::Utf8Path;
use serde::{Deserialize, Serialize};

/// Supported artifact metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricKind {
    /// Exact final artifact file size.
    ArtifactFileSize,
    /// Section sizes reported by `cargo size`/LLVM tools.
    SectionSize,
    /// Symbol and crate attribution reported by `cargo bloat`.
    SymbolAttribution,
    /// LLVM IR line attribution reported by `cargo llvm-lines`.
    LlvmLines,
}

/// Availability of a metric collector in the current environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MetricCapability {
    /// Collector is built in and always available.
    BuiltIn,
    /// External collector is installed.
    Available { version: String },
    /// External collector is not installed or not usable.
    Unsupported { reason: String },
}

/// Typed outcome from one metric collector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MetricOutcome {
    /// Collector produced a byte measurement.
    Bytes { value: u64 },
    /// Collector produced structured JSON attribution data.
    Json { value: serde_json::Value },
    /// Collector produced structured numeric rows whose unit is defined by the metric kind.
    Records { value: Vec<MetricRecord> },
    /// Collector is unavailable in the current environment.
    Unsupported { reason: String },
    /// Collector was available but collection failed.
    Failed { error: String },
}

/// One normalized external metric row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricRecord {
    /// Symbol, crate, section, or attribution label.
    pub name: String,
    /// Numeric value in bytes for size metrics or lines for LLVM-line metrics.
    pub value: u64,
}

/// Metric collector identity, capability, and outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricReport {
    /// Metric semantics.
    pub kind: MetricKind,
    /// Collector command or built-in identity.
    pub collector: String,
    /// Collector availability and version.
    pub capability: MetricCapability,
    /// Collection result.
    pub outcome: MetricOutcome,
}

/// Collects the mandatory exact artifact file-size metric.
#[must_use]
pub fn artifact_file_size(path: &Utf8Path, known_size: u64) -> MetricReport {
    MetricReport {
        kind: MetricKind::ArtifactFileSize,
        collector: "bloaty".to_owned(),
        capability: MetricCapability::BuiltIn,
        outcome: fs::metadata(path).map_or_else(
            |error| MetricOutcome::Failed {
                error: error.to_string(),
            },
            |metadata| {
                if metadata.len() == known_size {
                    MetricOutcome::Bytes { value: known_size }
                } else {
                    MetricOutcome::Failed {
                        error: "artifact changed between build resolution and metric collection"
                            .to_owned(),
                    }
                }
            },
        ),
    }
}

/// Detects optional external metric collectors without requiring them for built-in analysis.
#[must_use]
pub fn optional_capabilities() -> Vec<MetricReport> {
    [
        (
            MetricKind::SectionSize,
            "size",
            "cargo-size (section byte sizes)",
        ),
        (
            MetricKind::SymbolAttribution,
            "bloat",
            "cargo-bloat (symbol/crate byte attribution)",
        ),
        (
            MetricKind::LlvmLines,
            "llvm-lines",
            "cargo-llvm-lines (LLVM IR line attribution; not bytes)",
        ),
    ]
    .into_iter()
    .map(|(kind, command, collector)| capability(kind, command, collector))
    .collect()
}

/// Parses supported external collector JSON into normalized typed records.
///
/// Accepted forms are an array of objects, an object containing a `data` array, or newline-
/// delimited JSON objects. Labels use `name`, `symbol`, `crate`, or `section`; numeric values use
/// `bytes`, `size`, or `lines`.
///
/// # Errors
///
/// * The input is invalid JSON or does not contain supported label/value fields
pub fn parse_external_output(kind: MetricKind, output: &str) -> Result<MetricOutcome, String> {
    let values = serde_json::from_str::<serde_json::Value>(output)
        .map(|value| match value {
            serde_json::Value::Array(values) => values,
            serde_json::Value::Object(mut object) => object
                .remove("data")
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_else(|| vec![serde_json::Value::Object(object)]),
            value => vec![value],
        })
        .or_else(|_| {
            output
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(serde_json::from_str)
                .collect::<Result<Vec<serde_json::Value>, _>>()
        })
        .map_err(|error| format!("invalid collector JSON: {error}"))?;
    let mut records = Vec::with_capacity(values.len());
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| "collector record must be an object".to_owned())?;
        let name = ["name", "symbol", "crate", "section"]
            .into_iter()
            .find_map(|key| object.get(key).and_then(serde_json::Value::as_str))
            .ok_or_else(|| "collector record has no supported label field".to_owned())?;
        let fields = match kind {
            MetricKind::LlvmLines => ["lines", "size", "bytes"],
            _ => ["bytes", "size", "lines"],
        };
        let value = fields
            .into_iter()
            .find_map(|key| object.get(key).and_then(serde_json::Value::as_u64))
            .ok_or_else(|| "collector record has no supported numeric field".to_owned())?;
        records.push(MetricRecord {
            name: name.to_owned(),
            value,
        });
    }
    Ok(MetricOutcome::Records { value: records })
}

/// Executes an available external collector and parses its supported structured output.
///
/// The supplied arguments must select an output mode supported by [`parse_external_output`].
#[must_use]
pub fn run_external_collector(kind: MetricKind, command: &str, args: &[String]) -> MetricReport {
    let mut report = capability(kind, command, &format!("cargo-{command}"));
    let MetricCapability::Available { .. } = &report.capability else {
        return report;
    };
    let output = Command::new("cargo").arg(command).args(args).output();
    report.outcome = match output {
        Ok(output) if output.status.success() => {
            match parse_external_output(kind, &String::from_utf8_lossy(&output.stdout)) {
                Ok(outcome) => outcome,
                Err(error) => MetricOutcome::Failed { error },
            }
        }
        Ok(output) => MetricOutcome::Failed {
            error: format!(
                "cargo {command} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        },
        Err(error) => MetricOutcome::Failed {
            error: error.to_string(),
        },
    };
    report
}

fn capability(kind: MetricKind, command: &str, collector: &str) -> MetricReport {
    let output = Command::new("cargo").args([command, "--version"]).output();
    match output {
        Ok(output) if output.status.success() => MetricReport {
            kind,
            collector: collector.to_owned(),
            capability: MetricCapability::Available {
                version: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            },
            outcome: MetricOutcome::Unsupported {
                reason: "collector execution was not requested".to_owned(),
            },
        },
        Ok(output) => MetricReport {
            kind,
            collector: collector.to_owned(),
            capability: MetricCapability::Unsupported {
                reason: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            },
            outcome: MetricOutcome::Unsupported {
                reason: format!("cargo {command} is unavailable"),
            },
        },
        Err(error) => MetricReport {
            kind,
            collector: collector.to_owned(),
            capability: MetricCapability::Unsupported {
                reason: error.to_string(),
            },
            outcome: MetricOutcome::Unsupported {
                reason: format!("cargo {command} could not be started"),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_metric_verifies_exact_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("artifact");
        fs::write(&path, b"artifact").unwrap();
        let path = Utf8Path::from_path(&path).unwrap();
        let report = artifact_file_size(path, 8);
        assert_eq!(report.outcome, MetricOutcome::Bytes { value: 8 });
    }

    #[test]
    fn unavailable_external_collector_does_not_report_success() {
        let report = run_external_collector(
            MetricKind::SectionSize,
            "definitely-not-a-real-cargo-subcommand",
            &[],
        );
        assert!(matches!(
            report.capability,
            MetricCapability::Unsupported { .. }
        ));
        assert!(matches!(report.outcome, MetricOutcome::Unsupported { .. }));
    }

    #[test]
    fn parses_json_array_and_jsonl_collector_output() {
        assert_eq!(
            parse_external_output(
                MetricKind::SymbolAttribution,
                r#"[{"symbol":"main","bytes":42}]"#,
            )
            .unwrap(),
            MetricOutcome::Records {
                value: vec![MetricRecord {
                    name: "main".to_owned(),
                    value: 42,
                }],
            }
        );
        assert_eq!(
            parse_external_output(
                MetricKind::LlvmLines,
                "{\"name\":\"first\",\"lines\":2}\n{\"name\":\"second\",\"lines\":3}",
            )
            .unwrap(),
            MetricOutcome::Records {
                value: vec![
                    MetricRecord {
                        name: "first".to_owned(),
                        value: 2,
                    },
                    MetricRecord {
                        name: "second".to_owned(),
                        value: 3,
                    },
                ],
            }
        );
    }

    #[test]
    fn optional_collectors_have_explicit_capabilities() {
        let reports = optional_capabilities();
        assert_eq!(reports.len(), 3);
        assert!(reports.iter().all(|report| matches!(
            report.capability,
            MetricCapability::Available { .. } | MetricCapability::Unsupported { .. }
        )));
    }
}
