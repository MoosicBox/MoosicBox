//! Lossless scheduling of feature cases onto bounded CI jobs.
//!
//! Scheduling never changes a case. Only identical setup identities can share a
//! runner; each case is executed separately in an isolated checkout.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::BoxError;

/// A complete, independently executable feature configuration.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Case {
    /// Stable identity within the canonical plan, including duplicate cases.
    pub id: String,
    /// Original execution metadata, without scheduler-owned fields.
    pub package: Map<String, Value>,
}

/// Shared setup and a nonempty list of unchanged cases.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Job {
    /// Cases assigned to this runner, in execution order.
    pub cases: Vec<Case>,
    /// Identical setup metadata (features are intentionally not represented here).
    #[serde(flatten)]
    pub setup: Map<String, Value>,
}

/// Allocate cases deterministically, balancing case counts within compatible setups.
///
/// Custom setup scripts and explicit `freshRunner` cases are not stacked. The
/// caller must execute each case in a fresh checkout and process environment.
///
/// # Errors
///
/// * The job limit is zero or execution metadata is invalid.
/// * Incompatible setups require more jobs than the limit.
/// * JSON serialization fails.
pub fn schedule(mut packages: Vec<Map<String, Value>>, limit: usize) -> Result<Vec<Job>, BoxError> {
    if limit == 0 {
        return Err("max-jobs must be greater than zero".into());
    }
    for package in &packages {
        if package.contains_key("cases") || package.contains_key("caseId") {
            return Err("Input contains scheduler-owned fields".into());
        }
        for field in ["name", "path", "os"] {
            if package
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(format!("Missing or invalid matrix {field}").into());
            }
        }
        if !package
            .get("features")
            .is_some_and(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
        {
            return Err("Matrix features must be an array of strings".into());
        }
    }
    packages.sort_by_cached_key(|p| serde_json::to_string(p).unwrap_or_default());
    let mut groups = BTreeMap::<String, Vec<Case>>::new();
    for (index, package) in packages.into_iter().enumerate() {
        let mut setup = package.clone();
        setup.remove("features");
        // Arbitrary setup scripts may mutate runner-global state. Be conservative
        // until the project explicitly provides a safe setup lifecycle.
        let isolated = setup.get("freshRunner").and_then(Value::as_bool) == Some(true)
            || ["ciSteps", "dependencies"].iter().any(|field| {
                setup
                    .get(*field)
                    .is_some_and(|v| v.as_str().is_none_or(|s| !s.is_empty()))
            })
            || setup.get("gitSubmodules").and_then(Value::as_bool) == Some(true);
        let key = serde_json::to_string(&(setup, isolated.then_some(index)))?;
        groups.entry(key).or_default().push(Case {
            id: format!("case-{index:06}"),
            package,
        });
    }
    if groups.len() > limit {
        return Err(format!("Cannot schedule {} incompatible setups in {limit} jobs; custom ciSteps and freshRunner cases require separate runners", groups.len()).into());
    }
    let groups = groups.into_values().collect::<Vec<_>>();
    let mut slots = vec![1_usize; groups.len()];
    let total = groups.iter().map(Vec::len).sum::<usize>();
    for _ in groups.len()..limit.min(total) {
        // Largest work-per-slot ratio first; deterministic group-order tie break.
        let best = (0..groups.len())
            .filter(|&i| slots[i] < groups[i].len())
            .max_by(|&a, &b| {
                (groups[a].len() * slots[b])
                    .cmp(&(groups[b].len() * slots[a]))
                    .then_with(|| b.cmp(&a))
            })
            .ok_or("Missing schedulable group")?;
        slots[best] += 1;
    }
    let mut jobs = Vec::new();
    for (group, count) in groups.into_iter().zip(slots) {
        let mut setup = group[0].package.clone();
        setup.remove("features");
        let mut buckets = vec![Vec::new(); count];
        for (index, case) in group.into_iter().enumerate() {
            buckets[index % count].push(case);
        }
        jobs.extend(buckets.into_iter().map(|cases| Job {
            setup: setup.clone(),
            cases,
        }));
    }
    Ok(jobs)
}

/// A runner's terminal receipt for a case. Missing receipts never imply success.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    /// The complete unchanged case, not just its identity or feature union.
    pub case: Case,
    /// Exit status after execution and finalization.
    pub exit_code: i32,
}

/// Reconcile all planned cases against exactly one successful receipt each.
///
/// # Errors
///
/// * A case is missing, duplicated, unexpected, changed, or failed.
pub fn verify(jobs: &[Job], receipts: &[Receipt]) -> Result<(), BoxError> {
    let mut expected = BTreeMap::new();
    for case in jobs.iter().flat_map(|job| &job.cases) {
        if expected.insert(&case.id, case).is_some() {
            return Err("Duplicate planned case ID".into());
        }
    }
    let mut seen = BTreeSet::new();
    for receipt in receipts {
        if !seen.insert(&receipt.case.id)
            || expected.get(&receipt.case.id).copied() != Some(&receipt.case)
        {
            return Err(
                format!("Unexpected, duplicate, or changed case {}", receipt.case.id).into(),
            );
        }
        if receipt.exit_code != 0 {
            return Err(format!("Case {} failed ({})", receipt.case.id, receipt.exit_code).into());
        }
    }
    if seen.len() != expected.len() {
        return Err(format!(
            "Incomplete execution: {} of {} cases have receipts",
            seen.len(),
            expected.len()
        )
        .into());
    }
    Ok(())
}
