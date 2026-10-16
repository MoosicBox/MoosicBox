//! Deterministic evidence-size and context-preservation regression tests.
//!
//! These are bounded structural budgets, not timing benchmarks or compiler proofs.
#![cfg(feature = "machete")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier::machete::{
    DependencyEvidence, DependencyKind, DependencyStatus, EvidenceKind, MacheteOptions,
    MacheteReport, analyze_machete,
};
use std::{collections::BTreeSet, fmt::Write as _, fs, path::Path};

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn package(root: &Path, declarations: &str) {
    write(
        root,
        "Cargo.toml",
        &format!(
            "[package]\nname='performance_fixture'\nversion='0.1.0'\nedition='2021'\n{declarations}"
        ),
    );
}

fn analyze(root: &Path) -> MacheteReport {
    analyze_machete(&MacheteOptions {
        manifest_path: Some(root.join("Cargo.toml")),
        ..MacheteOptions::default()
    })
    .unwrap()
}

fn assert_no_exact_duplicates(report: &MacheteReport) {
    // Serialize the complete record so no context, uncertainty or provenance field
    // can accidentally be omitted from this regression test's identity key.
    let records = report
        .evidence
        .iter()
        .map(|record| serde_json::to_string(record).unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        records.len(),
        report.evidence.len(),
        "repeated traversal must not inflate identical report evidence"
    );
    assert!(
        report.evidence.windows(2).all(|pair| pair[0] < pair[1]),
        "canonical report evidence must have deterministic strict ordering"
    );
}

#[test]
fn evidence_ordering_keeps_every_identity_field_and_is_idempotent() {
    let base = DependencyEvidence {
        manifest_path: "Cargo.toml".into(),
        file: "src/lib.rs".into(),
        line: 1,
        column: 1,
        kind: EvidenceKind::Direct,
        source_target: DependencyKind::Normal,
        source_target_role: "LIBRARY".into(),
        source_target_name: Some("library".into()),
        dependency_kind: DependencyKind::Normal,
        target: None,
        alias: "dep".into(),
        resolved_package: "dependency".into(),
        conditions: Vec::new(),
        uncertainty: Vec::new(),
    };
    // Each variant differs in exactly one field. Canonicalization must retain
    // all variants, including unresolved provenance rather than hiding it.
    let variants = vec![
        base.clone(),
        DependencyEvidence {
            manifest_path: "other/Cargo.toml".into(),
            ..base.clone()
        },
        DependencyEvidence {
            file: "src/other.rs".into(),
            ..base.clone()
        },
        DependencyEvidence {
            line: 2,
            ..base.clone()
        },
        DependencyEvidence {
            column: 2,
            ..base.clone()
        },
        DependencyEvidence {
            kind: EvidenceKind::Import,
            ..base.clone()
        },
        DependencyEvidence {
            source_target: DependencyKind::Dev,
            ..base.clone()
        },
        DependencyEvidence {
            source_target_role: "BINARY".into(),
            ..base.clone()
        },
        DependencyEvidence {
            source_target_name: Some("other".into()),
            ..base.clone()
        },
        DependencyEvidence {
            dependency_kind: DependencyKind::Dev,
            ..base.clone()
        },
        DependencyEvidence {
            target: Some("cfg(unix)".into()),
            ..base.clone()
        },
        DependencyEvidence {
            alias: "renamed".into(),
            ..base.clone()
        },
        DependencyEvidence {
            resolved_package: "other".into(),
            ..base.clone()
        },
        DependencyEvidence {
            conditions: vec!["unix".into()],
            ..base.clone()
        },
        DependencyEvidence {
            uncertainty: vec!["unexpanded macro".into()],
            ..base
        },
    ];
    assert_eq!(variants.len(), 15);
    let mut canonical = variants.clone();
    canonical.extend(variants.iter().rev().cloned());
    canonical.sort_unstable();
    canonical.dedup();
    assert_eq!(canonical.len(), variants.len());
    let once = serde_json::to_vec(&canonical).unwrap();
    canonical.sort_unstable();
    canonical.dedup();
    assert_eq!(serde_json::to_vec(&canonical).unwrap(), once);
}

#[test]
fn nested_modules_have_linear_unique_evidence_not_traversal_depth_evidence() {
    // Every source is both conventionally discoverable and reachable from its
    // parents. A fresh visited set per source used to yield triangular output.
    for depth in [8, 32] {
        let directory = tempfile::tempdir().unwrap();
        package(directory.path(), "[dependencies]\ndep='1.0.0'\n");
        let mut relative = String::from("src/");
        for level in 0..depth {
            let filename = if level == 0 { "lib.rs" } else { "node.rs" };
            let module = if level + 1 < depth { "mod node;\n" } else { "" };
            write(
                directory.path(),
                &format!("{relative}{filename}"),
                &format!("{module}pub fn run() {{ dep::call(); }}\n"),
            );
            if level > 0 {
                relative.push_str("node/");
            }
        }
        let report = analyze(directory.path());
        assert_no_exact_duplicates(&report);
        let direct = report
            .evidence
            .iter()
            .filter(|record| record.alias == "dep" && record.kind == EvidenceKind::Direct)
            .collect::<Vec<_>>();
        assert_eq!(direct.len(), depth);
        assert_eq!(
            direct
                .iter()
                .map(|record| &record.file)
                .collect::<BTreeSet<_>>()
                .len(),
            depth
        );
        assert_eq!(report.dependencies.len(), 1);
        assert_eq!(report.dependencies[0].status, DependencyStatus::Used);
        assert!(
            report.complete,
            "simple module tree must resolve: {:?}",
            report.coverage.gaps
        );
        assert!(!directory.path().join("target").exists());
        assert!(!directory.path().join("Cargo.lock").exists());
    }
}

#[test]
fn evidence_dedup_preserves_distinct_source_positions_and_conditions() {
    let directory = tempfile::tempdir().unwrap();
    package(directory.path(), "[dependencies]\ndep='1.0.0'\n");
    write(
        directory.path(),
        "src/lib.rs",
        "#[cfg(unix)]\npub fn first() { dep::call(); dep::call(); }\n\
         #[cfg(windows)]\npub fn second() { dep::call(); }\n",
    );
    let report = analyze(directory.path());
    assert_no_exact_duplicates(&report);
    let direct = report
        .evidence
        .iter()
        .filter(|record| record.alias == "dep" && record.kind == EvidenceKind::Direct)
        .collect::<Vec<_>>();
    assert_eq!(direct.len(), 3);
    assert_eq!(
        direct
            .iter()
            .map(|record| (record.line, record.column))
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    assert_eq!(
        direct
            .iter()
            .map(|record| &record.conditions)
            .collect::<BTreeSet<_>>()
            .len(),
        2,
        "mutually conditional observations are different evidence contexts"
    );
    assert!(direct.iter().all(|record| !record.conditions.is_empty()));
}

#[test]
fn evidence_dedup_preserves_target_specific_declarations() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "[target.'cfg(unix)'.dependencies]\ndep='1.0.0'\n\
         [target.'cfg(windows)'.dependencies]\ndep='1.0.0'\n",
    );
    write(
        directory.path(),
        "src/lib.rs",
        "pub fn run() { dep::call(); }\n",
    );
    let report = analyze(directory.path());
    assert_no_exact_duplicates(&report);
    let direct = report
        .evidence
        .iter()
        .filter(|record| record.alias == "dep" && record.kind == EvidenceKind::Direct)
        .collect::<Vec<_>>();
    assert_eq!(direct.len(), 2);
    assert_eq!(
        direct
            .iter()
            .map(|record| record.target.as_deref())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([Some("cfg(unix)"), Some("cfg(windows)")])
    );
    assert!(direct.iter().all(|record| record.line == 1));
}

#[test]
fn evidence_dedup_preserves_declaration_and_source_categories() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "[dependencies]\ndep='1.0.0'\n[dev-dependencies]\ndep='1.0.0'\n\
         [build-dependencies]\ndep='1.0.0'\n",
    );
    for path in ["src/lib.rs", "tests/integration.rs", "build.rs"] {
        write(directory.path(), path, "pub fn run() { dep::call(); }\n");
    }
    let report = analyze(directory.path());
    assert_no_exact_duplicates(&report);
    let categories = report
        .evidence
        .iter()
        .filter(|record| record.alias == "dep" && record.kind == EvidenceKind::Direct)
        .map(|record| (record.dependency_kind, record.source_target))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        categories,
        BTreeSet::from([
            (DependencyKind::Normal, DependencyKind::Normal),
            (DependencyKind::Normal, DependencyKind::Dev),
            (DependencyKind::Dev, DependencyKind::Dev),
            (DependencyKind::Build, DependencyKind::Build),
        ])
    );
}

#[test]
fn evidence_join_growth_is_bounded_by_actual_matching_observations() {
    // Widen both declarations and references. Scanning every observation once per
    // declaration must not manufacture a Cartesian-product report. No timing
    // threshold: this remains stable on slow and contended CI hosts.
    for width in [16, 64] {
        let directory = tempfile::tempdir().unwrap();
        let mut declarations = String::from("[dependencies]\n");
        let mut source = String::from("pub fn run() {\n");
        for index in 0..width {
            writeln!(declarations, "dep_{index}='1.0.0'").unwrap();
            writeln!(source, "dep_{index}::call();").unwrap();
        }
        source.push_str("}\n");
        package(directory.path(), &declarations);
        write(directory.path(), "src/lib.rs", &source);
        let report = analyze(directory.path());
        assert_no_exact_duplicates(&report);
        assert_eq!(report.dependencies.len(), width);
        assert_eq!(report.evidence.len(), width);
        assert!(
            report
                .dependencies
                .iter()
                .all(|record| record.status == DependencyStatus::Used)
        );
        assert!(
            report
                .evidence
                .iter()
                .all(|record| record.uncertainty.is_empty())
        );
        // Check the entire wire result, including evidence ordering and coverage.
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&analyze(directory.path())).unwrap()
        );
    }
}

#[test]
fn unresolved_expansion_cannot_satisfy_the_evidence_size_budget_by_hiding_gaps() {
    let directory = tempfile::tempdir().unwrap();
    package(
        directory.path(),
        "[dependencies]\nobserved='1.0.0'\nunverified='1.0.0'\n",
    );
    write(
        directory.path(),
        "src/lib.rs",
        "generate_dependency_paths!();\npub fn run() { observed::call(); }\n",
    );
    let report = analyze(directory.path());
    assert_no_exact_duplicates(&report);
    assert!(!report.complete);
    assert_ne!(report.coverage.gaps.len(), 0);
    let unverified = report
        .dependencies
        .iter()
        .find(|record| record.dependency == "unverified")
        .unwrap();
    assert_eq!(unverified.status, DependencyStatus::Incomplete);
    assert!(
        report
            .evidence
            .iter()
            .any(|record| record.alias == "observed"),
        "uncertain expansion must not erase independently observed usage"
    );
}

#[test]
fn repeated_analysis_observes_source_edits_and_missing_module_arrival() {
    let directory = tempfile::tempdir().unwrap();
    package(directory.path(), "[dependencies]\ndep='1.0.0'\n");
    write(directory.path(), "src/lib.rs", "mod child;\n");
    let missing = analyze(directory.path());
    assert!(
        !missing.complete,
        "a missing module is not a proof of absence"
    );
    write(
        directory.path(),
        "src/child.rs",
        "pub fn run() { dep::first(); }\n",
    );
    let arrived = analyze(directory.path());
    assert!(arrived.complete, "{:?}", arrived.coverage.gaps);
    assert_eq!(arrived.dependencies[0].status, DependencyStatus::Used);
    assert_no_exact_duplicates(&arrived);
    let before = arrived
        .evidence
        .iter()
        .map(|record| record.line)
        .collect::<Vec<_>>();
    write(
        directory.path(),
        "src/child.rs",
        "\n\npub fn run() { dep::second(); }\n",
    );
    let edited = analyze(directory.path());
    assert_no_exact_duplicates(&edited);
    let after = edited
        .evidence
        .iter()
        .map(|record| record.line)
        .collect::<Vec<_>>();
    assert_ne!(
        before, after,
        "source locations must not come from stale syntax"
    );
    fs::remove_file(directory.path().join("src/child.rs")).unwrap();
    assert!(!analyze(directory.path()).complete);
}
