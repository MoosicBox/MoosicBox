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

/// Auditable allocation statistics; counts describe cases, not feature unions.
#[derive(Debug, Serialize)]
pub struct ScheduleSummary {
    /// Number of unchanged input cases, including duplicates.
    pub case_count: usize,
    /// Requested execution-job budget, independent of matrix transport limits.
    pub requested_jobs: usize,
    /// Smallest safe allocation under the current setup isolation policy.
    pub minimum_jobs: usize,
    /// Budget after explicitly permitted isolation overflow.
    pub effective_jobs: usize,
    /// Actual allocated jobs (never exceeds the number of cases).
    pub execution_jobs: usize,
    /// Jobs containing exactly one case.
    pub singleton_jobs: usize,
    /// Jobs containing multiple independently executed cases.
    pub stacked_jobs: usize,
    /// Counts of cases requiring isolation by reason; reasons can overlap.
    pub isolation_reasons: BTreeMap<String, usize>,
}

fn isolation_reasons(setup: &Map<String, Value>) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if setup.get("freshRunner").and_then(Value::as_bool) == Some(true) {
        reasons.push("freshRunner");
    }
    for field in ["ciSteps", "dependencies"] {
        if setup
            .get(field)
            .is_some_and(|v| v.as_str().is_none_or(|s| !s.is_empty()))
        {
            reasons.push(field);
        }
    }
    if setup.get("gitSubmodules").and_then(Value::as_bool) == Some(true) {
        reasons.push("gitSubmodules");
    }
    reasons
}

// Shared identity construction is independent of allocation and transport.
fn prepare_cases(mut packages: Vec<Map<String, Value>>) -> Result<Vec<Case>, BoxError> {
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
    Ok(packages
        .into_iter()
        .enumerate()
        .map(|(index, package)| Case {
            id: format!("case-{index:06}"),
            package,
        })
        .collect())
}

/// Build a deterministic job plan with exactly one unchanged case per runner.
/// No scheduling budget or stacking policy is applied.
///
/// # Errors
///
/// * Execution metadata is invalid or contains scheduler-owned fields.
pub fn plan(packages: Vec<Map<String, Value>>) -> Result<(Vec<Job>, ScheduleSummary), BoxError> {
    let cases = prepare_cases(packages)?;
    let count = cases.len();
    let mut reasons = BTreeMap::new();
    let jobs = cases
        .into_iter()
        .map(|case| {
            let mut setup = case.package.clone();
            setup.remove("features");
            for reason in isolation_reasons(&setup) {
                *reasons.entry(reason.to_owned()).or_insert(0) += 1;
            }
            Job {
                setup,
                cases: vec![case],
            }
        })
        .collect();
    Ok((
        jobs,
        ScheduleSummary {
            case_count: count,
            requested_jobs: count,
            minimum_jobs: count,
            effective_jobs: count,
            execution_jobs: count,
            singleton_jobs: count,
            stacked_jobs: 0,
            isolation_reasons: reasons,
        },
    ))
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
pub fn schedule(packages: Vec<Map<String, Value>>, limit: usize) -> Result<Vec<Job>, BoxError> {
    Ok(schedule_with_summary(packages, limit, false)?.0)
}

/// Schedule unchanged cases, optionally exceeding the budget only for isolation.
///
/// When permitted, overflow raises the budget to the minimum safe group count,
/// never to the case count. Compatible cases still stack within that budget.
/// Arbitrary dependency/setup scripts remain isolated: identical text is not
/// evidence that runner-global mutations are repeatable or safe to share.
///
/// # Errors
///
/// * Returns the validation and allocation errors documented by [`schedule`].
pub fn schedule_with_summary(
    packages: Vec<Map<String, Value>>,
    limit: usize,
    allow_isolation_overflow: bool,
) -> Result<(Vec<Job>, ScheduleSummary), BoxError> {
    if limit == 0 {
        return Err("max-jobs must be greater than zero".into());
    }
    let cases = prepare_cases(packages)?;
    let requested_jobs = limit;
    let mut reason_counts = BTreeMap::new();
    let mut groups = BTreeMap::<String, Vec<Case>>::new();
    for (index, case) in cases.into_iter().enumerate() {
        let package = &case.package;
        let mut setup = package.clone();
        setup.remove("features");
        // Arbitrary setup scripts may mutate runner-global state. Be conservative
        // until the project explicitly provides a safe setup lifecycle.
        let reasons = isolation_reasons(&setup);
        let isolated = !reasons.is_empty();
        for reason in reasons {
            *reason_counts.entry(reason.to_owned()).or_insert(0) += 1;
        }
        let key = serde_json::to_string(&(setup, isolated.then_some(index)))?;
        groups.entry(key).or_default().push(case);
    }
    let minimum_jobs = groups.len();
    let limit = if allow_isolation_overflow {
        limit.max(minimum_jobs)
    } else {
        limit
    };
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
    let singleton_jobs = jobs.iter().filter(|job| job.cases.len() == 1).count();
    let summary = ScheduleSummary {
        case_count: total,
        requested_jobs,
        minimum_jobs,
        effective_jobs: limit,
        execution_jobs: jobs.len(),
        singleton_jobs,
        stacked_jobs: jobs.len() - singleton_jobs,
        isolation_reasons: reason_counts,
    };
    Ok((jobs, summary))
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
