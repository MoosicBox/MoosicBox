//! Strict merging of independently measured feature shards.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};

use crate::{AnalysisReport, ComparisonKey, ScenarioStatus};

/// Merges shards with identical provenance and baseline measurements.
///
/// `expected` must contain every planned comparison name, including named scenarios.
/// Failed comparison outcomes are preserved; a failed baseline cannot establish comparability.
///
/// # Errors
///
/// * No shards, incompatible provenance, failed or differing baselines
/// * Duplicate, missing, or unexpected comparison scenarios
pub fn merge_reports(
    reports: Vec<AnalysisReport>,
    expected: &BTreeSet<String>,
) -> Result<AnalysisReport> {
    let mut reports = reports.into_iter();
    let mut merged = reports
        .next()
        .ok_or_else(|| anyhow::anyhow!("no shard reports"))?;
    let key = ComparisonKey::from(&merged);
    let ScenarioStatus::Success {
        measurement: baseline,
    } = &merged.baseline.outcome
    else {
        bail!("cannot merge a failed baseline");
    };
    let baseline_size = baseline.size_bytes;
    let mut scenarios = BTreeMap::new();
    for scenario in std::mem::take(&mut merged.comparisons) {
        ensure!(
            scenarios
                .insert(scenario.scenario.name.clone(), scenario)
                .is_none(),
            "duplicate scenario"
        );
    }
    for report in reports {
        ensure!(
            ComparisonKey::from(&report) == key && report.environment == merged.environment,
            "incompatible shard provenance"
        );
        ensure!(
            report.baseline.scenario == merged.baseline.scenario,
            "incompatible baseline configuration"
        );
        ensure!(
            matches!(&report.baseline.outcome, ScenarioStatus::Success { measurement } if measurement.size_bytes == baseline_size),
            "baseline failed or sizes disagree across shards"
        );
        merged.started_at = merged.started_at.min(report.started_at);
        for scenario in report.comparisons {
            let name = scenario.scenario.name.clone();
            ensure!(
                scenarios.insert(name.clone(), scenario).is_none(),
                "duplicate scenario: {name}"
            );
        }
    }
    let actual = scenarios.keys().cloned().collect::<BTreeSet<_>>();
    ensure!(
        &actual == expected,
        "scenario coverage mismatch: missing {:?}; unexpected {:?}",
        expected.difference(&actual).collect::<Vec<_>>(),
        actual.difference(expected).collect::<Vec<_>>()
    );
    merged.comparisons = scenarios.into_values().collect();
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shard(name: &str) -> AnalysisReport {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "started_at": 0, "package": "app", "target_name": "app",
            "target_kind": "binary", "profile": "release", "compilation_target": null,
            "environment": {"rustc":"rustc 1", "cargo":"cargo 1", "host_os":"linux", "host_arch":"x86_64", "git_revision":"abc", "git_dirty":false},
            "baseline": {"scenario":{"name":"baseline", "config":{"default_features":false,"features":[]}}, "status":"success", "measurement":{"artifact_path":"app","size_bytes":100,"delta_bytes":null,"delta_percent":null,"fresh":false}},
            "comparisons": [{"scenario":{"name":name,"config":{"default_features":false,"features":[name]}},"status":"failed","error":"build failed"}]
        })).unwrap()
    }

    #[test]
    fn merges_sorted_scenarios_and_preserves_failures() {
        let expected = BTreeSet::from(["a".to_owned(), "b".to_owned()]);
        let merged = merge_reports(vec![shard("b"), shard("a")], &expected).unwrap();
        assert_eq!(merged.comparisons[0].scenario.name, "a");
        assert!(matches!(
            merged.comparisons[0].outcome,
            ScenarioStatus::Failed { .. }
        ));
    }

    #[test]
    fn rejects_missing_duplicate_and_unexpected_scenarios() {
        let expected = BTreeSet::from(["a".to_owned(), "b".to_owned()]);
        assert!(merge_reports(vec![shard("a")], &expected).is_err());
        assert!(merge_reports(vec![shard("a"), shard("a")], &expected).is_err());
        assert!(merge_reports(vec![shard("a"), shard("c")], &expected).is_err());
        assert!(merge_reports(vec![], &expected).is_err());
    }

    #[test]
    fn rejects_baseline_drift_and_incompatible_provenance() {
        let expected = BTreeSet::from(["a".to_owned(), "b".to_owned()]);
        let mut other = shard("b");
        if let ScenarioStatus::Success { measurement } = &mut other.baseline.outcome {
            measurement.size_bytes += 1;
        }
        assert!(merge_reports(vec![shard("a"), other], &expected).is_err());
        let mut other = shard("b");
        other.environment.git_revision = Some("different".to_owned());
        assert!(merge_reports(vec![shard("a"), other], &expected).is_err());
        let mut other = shard("b");
        other.baseline.outcome = ScenarioStatus::Failed {
            error: "failed".to_owned(),
        };
        assert!(merge_reports(vec![shard("a"), other], &expected).is_err());
    }
}
