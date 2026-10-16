//! Deterministic offline audit tests. Synthetic evidence avoids duplicating the
//! imported source corpus; discovery/source tests own collection correctness.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier::machete::{
    DependencyKind,
    audit::{
        AuditDeclaration, AuditInput, AuditKind, AuditLocation, AuditSpecification,
        AuditSuppression, AuditUsage, AuditWorkspaceDependency, audit_machete_evidence,
    },
};
use std::{collections::BTreeSet, path::PathBuf};

fn location(package: &str, dependency: &str, kind: Option<DependencyKind>) -> AuditLocation {
    AuditLocation {
        path: PathBuf::from(format!("/workspace/{package}/Cargo.toml")),
        package: Some(package.into()),
        dependency: Some(dependency.into()),
        kind,
        target: None,
    }
}

fn specification() -> AuditSpecification {
    AuditSpecification {
        registry: "crates-io".into(),
        version: "1.0.0".into(),
        default_features: false,
        features: BTreeSet::new(),
    }
}

fn declaration(package: &str, dependency: &str, kind: DependencyKind) -> AuditDeclaration {
    AuditDeclaration {
        location: location(package, dependency, Some(kind)),
        dependency_package: dependency.into(),
        specification: Some(specification()),
        optional: false,
        workspace: false,
        selected: true,
        usage: AuditUsage::default(),
    }
}

fn suppression(pattern: &str) -> AuditSuppression {
    AuditSuppression {
        location: AuditLocation {
            path: "/workspace/clippier.toml".into(),
            package: None,
            dependency: Some(pattern.into()),
            kind: None,
            target: None,
        },
        pattern: pattern.into(),
        manifests: BTreeSet::from([PathBuf::from("/workspace/app/Cargo.toml")]),
        scope_complete: true,
        occurrences: None,
        reason: Some("runtime-generated integration".into()),
        review_after: None,
    }
}

fn workspace(dependency: &str) -> AuditWorkspaceDependency {
    AuditWorkspaceDependency {
        location: location("", dependency, None),
        dependency_package: dependency.into(),
        specification: Some(specification()),
    }
}

fn kinds(input: &AuditInput) -> Vec<AuditKind> {
    audit_machete_evidence(input)
        .unwrap()
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.kind)
        .collect()
}

#[test]
fn unmatched_exact_and_wildcard_with_complete_scope() {
    let input = AuditInput {
        suppressions: vec![suppression("missing"), suppression("absent*")],
        declarations: vec![declaration("app", "present", DependencyKind::Normal)],
        ..AuditInput::default()
    };
    assert_eq!(
        kinds(&input),
        [
            AuditKind::UnmatchedIgnore,
            AuditKind::UnmatchedIgnore,
            AuditKind::BroadSuppression
        ]
    );
    let report = audit_machete_evidence(&input).unwrap();
    assert_eq!(
        report.diagnostics[0].location.dependency.as_deref(),
        Some("absent*")
    );
    assert_eq!(
        report.diagnostics[1].location.dependency.as_deref(),
        Some("missing")
    );
}

#[test]
fn alias_matching_and_positive_ignored_usage_are_not_status_inference() {
    let mut item = declaration("app", "alias", DependencyKind::Normal);
    item.dependency_package = "actual-name".into();
    item.usage
        .production
        .insert("/workspace/app/src/lib.rs".into());
    item.selected = false;
    let input = AuditInput {
        declarations: vec![item],
        suppressions: vec![suppression("actual-*"), suppression("alias")],
        ..AuditInput::default()
    };
    assert_eq!(
        kinds(&input),
        [
            AuditKind::IgnoredButUsed,
            AuditKind::IgnoredButUsed,
            AuditKind::BroadSuppression
        ]
    );
    let report = audit_machete_evidence(&input).unwrap();
    assert_eq!(report.diagnostics[0].related.len(), 2);
}

#[test]
fn incomplete_scope_never_proves_unmatched_even_with_no_selected_matches() {
    let mut rule = suppression("unknown*");
    rule.scope_complete = false;
    let input = AuditInput {
        suppressions: vec![rule],
        ..AuditInput::default()
    };
    assert_eq!(
        kinds(&input),
        [
            AuditKind::BroadSuppression,
            AuditKind::SuppressionScopeUncertainty
        ]
    );
}

#[test]
fn same_manifest_rule_scope_intersects_ignore_glob_without_leaking_to_other_dependencies() {
    let foo = declaration("app", "foo", DependencyKind::Normal);
    let mut bar = declaration("app", "bar", DependencyKind::Normal);
    bar.usage
        .production
        .insert("/workspace/app/src/lib.rs".into());
    let mut rule = suppression("bar");
    rule.occurrences = Some(vec![foo.location.clone()]);
    let mut input = AuditInput {
        declarations: vec![foo, bar],
        suppressions: vec![rule],
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input), [AuditKind::UnmatchedIgnore]);
    input.suppressions[0].pattern = "b*".into();
    assert_eq!(
        kinds(&input),
        [AuditKind::UnmatchedIgnore, AuditKind::BroadSuppression]
    );
    input.suppressions[0].pattern = "*".into();
    assert_eq!(kinds(&input), [AuditKind::BroadSuppression]);
    input.suppressions[0].occurrences = Some(vec![input.declarations[1].location.clone()]);
    assert_eq!(
        kinds(&input),
        [AuditKind::IgnoredButUsed, AuditKind::BroadSuppression]
    );
    input.suppressions[0].occurrences = Some(Vec::new());
    assert_eq!(
        kinds(&input),
        [AuditKind::UnmatchedIgnore, AuditKind::BroadSuppression]
    );
    input.suppressions[0].occurrences = None;
    assert_eq!(
        kinds(&input),
        [AuditKind::IgnoredButUsed, AuditKind::BroadSuppression]
    );
}

#[test]
fn exact_occurrence_scope_preserves_dependency_kind_target_and_manifest_boundaries() {
    let normal = declaration("app", "dep", DependencyKind::Normal);
    let mut dev = declaration("app", "dep", DependencyKind::Dev);
    dev.usage
        .development
        .insert("/workspace/app/tests/test.rs".into());
    // Avoid unrelated redundancy advice; this test concerns suppression matching.
    dev.specification = None;
    let mut targeted = declaration("app", "dep", DependencyKind::Normal);
    targeted.location.target = Some("cfg(windows)".into());
    targeted
        .usage
        .production
        .insert("/workspace/app/src/windows.rs".into());
    let mut rule = suppression("dep");
    rule.occurrences = Some(vec![normal.location.clone()]);
    let mut input = AuditInput {
        declarations: vec![normal, dev, targeted],
        suppressions: vec![rule],
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input).len(), 0);
    input.suppressions[0].occurrences = Some(vec![input.declarations[2].location.clone()]);
    assert_eq!(kinds(&input), [AuditKind::IgnoredButUsed]);
    let report = audit_machete_evidence(&input).unwrap();
    assert!(
        report.diagnostics[0]
            .related
            .contains(&input.declarations[2].location)
    );
    assert!(
        !report.diagnostics[0]
            .related
            .contains(&input.declarations[1].location)
    );
    input.suppressions[0].manifests.clear();
    assert_eq!(kinds(&input), [AuditKind::UnmatchedIgnore]);
}

#[test]
fn multivalued_target_facts_do_not_disprove_other_features_or_atomic_widths() {
    use clippier::machete::{AnalysisProfile, ConditionValue, evaluate_condition};
    let profile = AnalysisProfile {
        cfg: vec![
            "target_feature=\"sse2\"".into(),
            "target_feature=\"avx\"".into(),
            "target_has_atomic=\"8\"".into(),
            "target_has_atomic=\"ptr\"".into(),
            "target_os=\"linux\"".into(),
        ],
        ..AnalysisProfile::default()
    };
    for condition in [
        "target_feature=\"sse2\"",
        "target_feature=\"avx\"",
        "target_has_atomic=\"8\"",
        "target_has_atomic=\"ptr\"",
    ] {
        assert_eq!(
            evaluate_condition(condition, &profile),
            ConditionValue::True
        );
    }
    for condition in [
        "target_feature=\"avx2\"",
        "target_has_atomic=\"64\"",
        "target_family=\"unix\"",
    ] {
        assert_eq!(
            evaluate_condition(condition, &profile),
            ConditionValue::Unknown
        );
    }
    assert_eq!(
        evaluate_condition("target_os=\"windows\"", &profile),
        ConditionValue::False
    );
}

#[test]
fn rule_scope_does_not_match_other_package_or_an_empty_scope() {
    let mut rule = suppression("present");
    rule.manifests.clear();
    let input = AuditInput {
        declarations: vec![declaration("other", "present", DependencyKind::Normal)],
        suppressions: vec![rule],
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input), [AuditKind::UnmatchedIgnore]);
}

#[test]
fn missing_rationale_and_due_dates_use_only_injected_clock() {
    let mut rule = suppression("dep");
    rule.reason = Some(" \n".into());
    rule.review_after = Some("2024-02-29".into());
    let mut input = AuditInput {
        declarations: vec![declaration("app", "dep", DependencyKind::Normal)],
        suppressions: vec![rule],
        review_date: Some("2024-03-01".into()),
        ..AuditInput::default()
    };
    assert_eq!(
        kinds(&input),
        [
            AuditKind::MissingSuppressionReason,
            AuditKind::SuppressionReviewOverdue
        ]
    );
    input.review_date = None;
    assert_eq!(kinds(&input), [AuditKind::MissingSuppressionReason]);
    input.suppressions[0].review_after = Some("2023-02-29".into());
    assert!(audit_machete_evidence(&input).is_err());
}

#[test]
fn invalid_globs_and_review_dates_are_errors_not_silent_no_matches() {
    let mut input = AuditInput {
        suppressions: vec![suppression("[")],
        ..AuditInput::default()
    };
    assert!(audit_machete_evidence(&input).is_err());
    input.suppressions.clear();
    for date in [
        "2024-00-01",
        "2024-01-32",
        "0000-01-01",
        "tomorrow",
        "2024-1-01",
    ] {
        input.review_date = Some(date.into());
        assert!(audit_machete_evidence(&input).is_err(), "{date}");
    }
}

#[test]
fn dev_only_requires_reliable_unsuppressed_target_evidence() {
    let mut item = declaration("app", "dep", DependencyKind::Normal);
    item.usage.targets_complete = true;
    item.usage
        .development
        .insert("/workspace/app/tests/test.rs".into());
    let mut input = AuditInput {
        declarations: vec![item],
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input), [AuditKind::NormalToDev]);
    input.declarations[0].usage.targets_complete = false;
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].usage.targets_complete = true;
    input.declarations[0]
        .usage
        .production
        .insert("/workspace/app/src/lib.rs".into());
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].usage.production.clear();
    input.declarations[0].optional = true;
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].optional = false;
    input.declarations[0].location.target = Some("cfg(unix)".into());
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].location.target = None;
    input.declarations[0].selected = false;
    assert_eq!(kinds(&input).len(), 0);
}

#[test]
fn workspace_reference_inventory_includes_unselected_consumers_and_all_kinds() {
    let mut item = declaration("excluded", "dep", DependencyKind::Dev);
    item.workspace = true;
    item.selected = false;
    item.location.target = Some("cfg(windows)".into());
    let mut input = AuditInput {
        declarations: vec![item],
        workspace_dependencies: vec![workspace("dep"), workspace("orphan")],
        reference_inventory_complete: true,
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input), [AuditKind::UnreferencedWorkspaceDeclaration]);
    assert_eq!(
        audit_machete_evidence(&input).unwrap().diagnostics[0]
            .location
            .dependency
            .as_deref(),
        Some("orphan")
    );
    input.reference_inventory_complete = false;
    assert_eq!(kinds(&input).len(), 0);
    input.reference_inventory_complete = true;
    input
        .excluded_directories
        .push(location("excluded", "directory", None));
    assert_eq!(kinds(&input), [AuditKind::DirectoryExclusionUncertainty]);
    assert!(
        !audit_machete_evidence(&input)
            .unwrap()
            .reference_inventory_complete
    );
}

#[test]
fn redundant_development_occurrence_requires_exact_effective_configuration() {
    let normal = declaration("app", "dep", DependencyKind::Normal);
    let dev = declaration("app", "dep", DependencyKind::Dev);
    let mut input = AuditInput {
        declarations: vec![normal, dev],
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input), [AuditKind::RedundantOccurrence]);
    input.declarations[1]
        .specification
        .as_mut()
        .unwrap()
        .features
        .insert("extra".into());
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[1].specification = Some(specification());
    input.declarations[0].location.target = Some("cfg(unix)".into());
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].location.target = None;
    input.declarations[0].optional = true;
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].optional = false;
    input.declarations[0].specification = None;
    assert_eq!(kinds(&input).len(), 0);
}

#[test]
fn consolidation_is_one_sorted_advisory_not_version_compatibility_guessing() {
    let first = declaration("a", "dep", DependencyKind::Normal);
    let second = declaration("b", "dep", DependencyKind::Normal);
    let mut input = AuditInput {
        declarations: vec![second, first],
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input), [AuditKind::WorkspaceConsolidation]);
    let report = audit_machete_evidence(&input).unwrap();
    assert_eq!(report.diagnostics[0].location.package.as_deref(), Some("a"));
    assert_eq!(
        report.diagnostics[0].related[0].package.as_deref(),
        Some("b")
    );
    input.declarations.reverse();
    assert_eq!(
        report.diagnostics,
        audit_machete_evidence(&input).unwrap().diagnostics
    );
    input.declarations[0]
        .specification
        .as_mut()
        .unwrap()
        .version = "^1.0.0".into();
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].specification = None;
    assert_eq!(kinds(&input).len(), 0);
}

#[test]
fn report_adapter_never_treats_feature_or_uncertain_observations_as_source_usage() {
    use clippier::machete::{AnalysisCoverage, DependencyEvidence, EvidenceKind, MacheteReport};
    let mut item = declaration("app", "alias", DependencyKind::Normal);
    item.usage.targets_complete = true;
    let evidence = DependencyEvidence {
        manifest_path: item.location.path.clone(),
        file: "/workspace/app/tests/example.rs".into(),
        line: 1,
        column: 1,
        kind: EvidenceKind::Feature,
        source_target: DependencyKind::Dev,
        source_target_role: "TEST".into(),
        source_target_name: Some("example".into()),
        dependency_kind: DependencyKind::Normal,
        target: None,
        alias: "alias".into(),
        resolved_package: "alias".into(),
        conditions: Vec::new(),
        uncertainty: Vec::new(),
    };
    let mut report = MacheteReport {
        root: "/workspace".into(),
        dependencies: Vec::new(),
        evidence: vec![evidence],
        coverage: AnalysisCoverage {
            precision: true,
            ..AnalysisCoverage::default()
        },
        diagnostics: Vec::new(),
        complete: true,
        applied: false,
        lockfile_preserved: true,
    };
    let fresh = AuditInput {
        declarations: vec![item],
        ..AuditInput::default()
    };
    let mut input = fresh.clone();
    input.add_report_evidence(&report);
    assert!(input.declarations[0].usage.development.is_empty());
    assert!(!input.declarations[0].usage.targets_complete);
    report.evidence[0].kind = EvidenceKind::Import;
    report.evidence[0]
        .uncertainty
        .push("unresolved scope".into());
    let mut input = fresh.clone();
    input.add_report_evidence(&report);
    assert!(input.declarations[0].usage.development.is_empty());
    report.evidence[0].uncertainty.clear();
    let mut input = fresh.clone();
    input.add_report_evidence(&report);
    assert_eq!(kinds(&input), [AuditKind::NormalToDev]);
    report.coverage.profile_limited = true;
    let mut input = fresh.clone();
    input.add_report_evidence(&report);
    assert!(!input.declarations[0].usage.development.is_empty());
    assert_eq!(kinds(&input).len(), 0);
    report.coverage.profile_limited = false;
    report.evidence[0].conditions.push("test".into());
    let mut input = fresh.clone();
    input.add_report_evidence(&report);
    assert_eq!(kinds(&input).len(), 0);
    report.evidence[0].conditions.clear();
    report.evidence[0].target = Some("cfg(windows)".into());
    let mut input = fresh;
    input.add_report_evidence(&report);
    assert!(input.declarations[0].usage.development.is_empty());
}

#[test]
fn workspace_configuration_difference_is_only_advisory_and_requires_comparable_specs() {
    let mut item = declaration("app", "dep", DependencyKind::Normal);
    item.specification.as_mut().unwrap().default_features = true;
    let mut input = AuditInput {
        declarations: vec![item],
        workspace_dependencies: vec![workspace("dep")],
        ..AuditInput::default()
    };
    assert_eq!(kinds(&input), [AuditKind::WorkspaceInconsistency]);
    input.declarations[0].selected = false;
    assert_eq!(kinds(&input).len(), 0);
    input.declarations[0].selected = true;
    input.workspace_dependencies[0].specification = None;
    assert_eq!(kinds(&input).len(), 0);
}
