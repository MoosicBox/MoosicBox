//! Advisory dependency hygiene over an explicitly supplied, offline evidence inventory.
//!
//! This module does not discover packages, run Cargo, read files, or change dependency
//! classifications. The discovery owner supplies unsuppressed evidence and declares
//! inventory boundaries. A finite set of profiles is never a global unused proof.

use super::{BoxError, DependencyKind, EvidenceKind, MacheteReport};
use globset::GlobBuilder;
use serde::Serialize;
use std::{collections::BTreeSet, path::PathBuf};

/// Stable declaration or configuration location, independent of diagnostic wording.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct AuditLocation {
    /// Manifest or configuration file, using the discovery owner's normalized path.
    pub path: PathBuf,
    /// Package name, if this is a package declaration.
    pub package: Option<String>,
    /// Dependency alias or suppression pattern.
    pub dependency: Option<String>,
    /// Dependency section, absent for configuration/workspace declarations.
    pub kind: Option<DependencyKind>,
    /// Cargo target predicate, without evaluating host cfg.
    pub target: Option<String>,
}

/// Source evidence collected with suppressions removed **in memory**.
#[derive(Debug, Clone, Default)]
pub struct AuditUsage {
    /// Actual source references in library or binary targets.
    pub production: BTreeSet<PathBuf>,
    /// Actual source references in tests, examples, benches, or cfg(test) code.
    pub development: BTreeSet<PathBuf>,
    /// Actual build-script references.
    pub build: BTreeSet<PathBuf>,
    /// All relevant source targets were classified reliably; false for unknown
    /// cfgs, expansions, generated source, inaccessible files, or deferred targets.
    /// This must not be inferred merely from the main report's `complete` flag.
    pub targets_complete: bool,
    /// Named profiles examined; descriptive only, not global coverage proof.
    pub profiles: BTreeSet<String>,
}

/// Comparable, resolved dependency configuration supplied by manifest discovery.
///
/// Populate only if source identity, version requirement, default features, and
/// requested features can be compared exactly. Leave `None` on the declaration
/// for unresolved inheritance, source paths, or unsupported Cargo syntax.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuditSpecification {
    /// Registry identity (including crates.io), not a relative path or Git URL.
    pub registry: String,
    /// Exact version requirement text; no semver compatibility inference is made.
    pub version: String,
    /// Effective default-feature setting.
    pub default_features: bool,
    /// Effective requested features.
    pub features: BTreeSet<String>,
}

/// One manifest occurrence, including occurrences outside the current selectors.
#[derive(Debug, Clone)]
pub struct AuditDeclaration {
    /// Stable occurrence identity.
    pub location: AuditLocation,
    /// Actual package name (not the alias).
    pub dependency_package: String,
    /// Effective configuration if safely comparable.
    pub specification: Option<AuditSpecification>,
    /// Whether Cargo marks this occurrence optional.
    pub optional: bool,
    /// Whether this occurrence has `workspace = true`.
    pub workspace: bool,
    /// Whether this occurrence is selected for advisory occurrence diagnostics.
    /// Unselected occurrences still participate in reference and pattern inventory.
    pub selected: bool,
    /// Independently collected, unsuppressed source evidence.
    pub usage: AuditUsage,
}

/// A suppression with its effective scope already resolved by configuration discovery.
#[derive(Debug, Clone)]
pub struct AuditSuppression {
    /// Config path and exact pattern; kind/target should be absent.
    pub location: AuditLocation,
    /// Glob matching dependency alias or actual package name.
    pub pattern: String,
    /// Applicable manifest paths, including unselected consumers. Empty means none,
    /// not all. Core resolves layered directory/package scope here.
    pub manifests: BTreeSet<PathBuf>,
    /// Optional exact occurrence scope, intersected with `manifests` and `pattern`.
    /// Use this for scoped rules after resolving package/dependency selectors,
    /// before matching the ignore glob. `None` means no occurrence restriction;
    /// `Some(Vec::new())` means no applicable declarations. Includes unselected
    /// occurrences and preserves dependency alias, kind, and target identity.
    pub occurrences: Option<Vec<AuditLocation>>,
    /// False if excluded directories or incomplete discovery may hide matches.
    pub scope_complete: bool,
    /// Human-maintained justification, if supplied by configuration.
    pub reason: Option<String>,
    /// Optional review deadline in YYYY-MM-DD format.
    pub review_after: Option<String>,
}

/// Workspace-owned dependency entry.
#[derive(Debug, Clone)]
pub struct AuditWorkspaceDependency {
    /// Workspace manifest and dependency alias.
    pub location: AuditLocation,
    /// Effective registry specification if safely comparable.
    pub specification: Option<AuditSpecification>,
    /// Actual package name.
    pub dependency_package: String,
}

/// Fully injectable input; callers retain responsibility for discovery completeness.
#[derive(Debug, Clone, Default)]
pub struct AuditInput {
    /// Every discovered declaration, before package/kind/dependency selectors.
    pub declarations: Vec<AuditDeclaration>,
    /// Effective suppressions, preserving their declaring location and scope.
    pub suppressions: Vec<AuditSuppression>,
    /// Workspace dependency entries, including entries with no consumers.
    pub workspace_dependencies: Vec<AuditWorkspaceDependency>,
    /// True only if all potential workspace consumers were inventoried, including
    /// unselected/excluded packages and all dependency kinds/target tables.
    pub reference_inventory_complete: bool,
    /// Locations of directory exclusions, including ignore-file exclusions.
    pub excluded_directories: Vec<AuditLocation>,
    /// Optional injected review date in YYYY-MM-DD format. No clock is consulted.
    pub review_date: Option<String>,
}

impl AuditInput {
    /// Merge positive source observations from a core report into inventory entries.
    ///
    /// This never discovers declarations or promotes completeness. The caller must
    /// supply the preselector inventory and initially establish reliable target
    /// coverage. Conditional, profile-limited, expansion, and unresolved observations
    /// revoke that coverage. Feature/implicit observations are not source references.
    pub fn add_report_evidence(&mut self, report: &MacheteReport) {
        for declaration in &mut self.declarations {
            if !report.complete
                || !report.coverage.precision
                || report.coverage.profile_limited
                || !report.coverage.gaps.is_empty()
            {
                declaration.usage.targets_complete = false;
            }
            if let Some(profile) = &report.coverage.profile {
                declaration.usage.profiles.insert(profile.clone());
            }
            for evidence in &report.evidence {
                if evidence.manifest_path != declaration.location.path
                    || Some(&evidence.alias) != declaration.location.dependency.as_ref()
                    || evidence.resolved_package != declaration.dependency_package
                    || Some(evidence.dependency_kind) != declaration.location.kind
                    || evidence.target != declaration.location.target
                {
                    continue;
                }
                if !evidence.conditions.is_empty() || !evidence.uncertainty.is_empty() {
                    declaration.usage.targets_complete = false;
                }
                if !matches!(
                    evidence.kind,
                    EvidenceKind::Direct | EvidenceKind::Import | EvidenceKind::Reexport
                ) {
                    declaration.usage.targets_complete = false;
                    continue;
                }
                if !evidence.uncertainty.is_empty() {
                    continue;
                }
                match evidence.source_target {
                    DependencyKind::Normal => &mut declaration.usage.production,
                    DependencyKind::Dev => &mut declaration.usage.development,
                    DependencyKind::Build => &mut declaration.usage.build,
                }
                .insert(evidence.file.clone());
            }
        }
    }
}

/// Advisory categories; none authorize a rewrite or classify a dependency unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditKind {
    /// Pattern has no declaration matches in a completely inventoried scope.
    UnmatchedIgnore,
    /// Suppressed declaration has an actual unsuppressed source reference.
    IgnoredButUsed,
    /// Wildcard can hide multiple or future dependency declarations.
    BroadSuppression,
    /// Exclusion prevents whole-workspace conclusions.
    DirectoryExclusionUncertainty,
    /// Pattern scope was not completely inventoried.
    SuppressionScopeUncertainty,
    /// Suppression lacks a nonblank justification.
    MissingSuppressionReason,
    /// Suppression's explicit review deadline precedes the injected review date.
    SuppressionReviewOverdue,
    /// Reliably classified source references occur only in development targets.
    NormalToDev,
    /// No workspace-inherited occurrence references this entry in a complete inventory.
    UnreferencedWorkspaceDeclaration,
    /// An unconditional normal occurrence subsumes an identical dev occurrence.
    RedundantOccurrence,
    /// A direct declaration differs from the same-name workspace configuration.
    WorkspaceInconsistency,
    /// Multiple packages independently repeat the same exact registry configuration.
    WorkspaceConsolidation,
}

/// Stable advisory identity: category plus primary location; related locations are
/// sorted and deduplicated. Text is explanatory, not a machine identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct AuditDiagnostic {
    /// Advisory category.
    pub kind: AuditKind,
    /// Exact primary manifest/configuration occurrence.
    pub location: AuditLocation,
    /// Matching declaration, source file, or related configuration locations.
    pub related: Vec<AuditLocation>,
    /// Conservative explanation and applicability caveats.
    pub message: String,
}

/// Separate advisory report; deliberately has no apply plan or unused status.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AuditReport {
    /// Deterministic diagnostics, sorted by category and location.
    pub diagnostics: Vec<AuditDiagnostic>,
    /// Whether the supplied inventory covers all workspace consumers. This says
    /// nothing about full compiler name resolution or unexamined profiles.
    pub reference_inventory_complete: bool,
}

fn emit(
    report: &mut AuditReport,
    kind: AuditKind,
    location: &AuditLocation,
    related: Vec<AuditLocation>,
    message: &str,
) {
    let mut related = related;
    related.sort();
    related.dedup();
    report.diagnostics.push(AuditDiagnostic {
        kind,
        location: location.clone(),
        related,
        message: message.into(),
    });
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..].parse::<u32>().unwrap_or(0);
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days
}

fn source_locations(declaration: &AuditDeclaration) -> Vec<AuditLocation> {
    declaration
        .usage
        .production
        .iter()
        .chain(&declaration.usage.development)
        .chain(&declaration.usage.build)
        .map(|path| AuditLocation {
            path: path.clone(),
            package: declaration.location.package.clone(),
            dependency: declaration.location.dependency.clone(),
            kind: None,
            target: None,
        })
        .collect()
}

/// Audit suppressions and manifest hygiene without filesystem access or execution.
///
/// Feed the existing discovery graph into [`AuditInput`], including unsuppressed
/// source evidence. Incomplete source evidence suppresses target-movement advice;
/// incomplete reference inventory suppresses unreferenced-workspace advice.
/// Recommendations always require manual review across supported profiles.
///
/// # Errors
///
/// * An ignore glob is invalid according to the core engine's glob syntax.
/// * An injected review date or configured deadline is not a valid ISO calendar date.
///
/// No files are opened or modified and no subprocess or network operation occurs.
pub fn audit_machete_evidence(input: &AuditInput) -> Result<AuditReport, BoxError> {
    if input
        .review_date
        .as_deref()
        .is_some_and(|value| !valid_date(value))
    {
        return Err("invalid audit review date; expected YYYY-MM-DD".into());
    }
    let mut report = AuditReport {
        diagnostics: Vec::new(),
        reference_inventory_complete: input.reference_inventory_complete
            && input.excluded_directories.is_empty(),
    };
    for exclusion in &input.excluded_directories {
        emit(
            &mut report,
            AuditKind::DirectoryExclusionUncertainty,
            exclusion,
            Vec::new(),
            "Directory exclusion limits audit coverage; no whole-workspace absence claim is made.",
        );
    }
    for suppression in &input.suppressions {
        audit_suppression(input, suppression, &mut report)?;
    }
    for declaration in input.declarations.iter().filter(|item| item.selected) {
        audit_declaration(input, declaration, &mut report);
    }
    audit_workspace(input, &mut report);
    report.diagnostics.sort();
    report.diagnostics.dedup();
    Ok(report)
}

fn audit_suppression(
    input: &AuditInput,
    suppression: &AuditSuppression,
    report: &mut AuditReport,
) -> Result<(), BoxError> {
    let glob = GlobBuilder::new(&suppression.pattern)
        .literal_separator(true)
        .build()?
        .compile_matcher();
    let matches: Vec<_> = input
        .declarations
        .iter()
        .filter(|declaration| {
            suppression.manifests.contains(&declaration.location.path)
                && suppression
                    .occurrences
                    .as_ref()
                    .is_none_or(|scope| scope.contains(&declaration.location))
                && (declaration
                    .location
                    .dependency
                    .as_ref()
                    .is_some_and(|key| glob.is_match(key))
                    || glob.is_match(&declaration.dependency_package))
        })
        .collect();
    if suppression.pattern.contains(['*', '?', '[', '{']) {
        emit(
            report,
            AuditKind::BroadSuppression,
            &suppression.location,
            matches.iter().map(|item| item.location.clone()).collect(),
            "Wildcard suppression can hide current or future declarations; prefer an explicitly justified narrow scope. Do not delete it based on finite profile observations.",
        );
    }
    if !suppression.scope_complete {
        emit(
            report,
            AuditKind::SuppressionScopeUncertainty,
            &suppression.location,
            Vec::new(),
            "Suppression scope is incomplete; absence of observed matches is not an unmatched-pattern finding.",
        );
    } else if matches.is_empty() {
        emit(
            report,
            AuditKind::UnmatchedIgnore,
            &suppression.location,
            Vec::new(),
            "No declaration matches this pattern in its fully inventoried scope; review configuration intent before changing it.",
        );
    }
    if suppression
        .reason
        .as_deref()
        .is_none_or(|reason| reason.trim().is_empty())
    {
        emit(
            report,
            AuditKind::MissingSuppressionReason,
            &suppression.location,
            Vec::new(),
            "Record why this suppression is needed; missing rationale is not evidence that it can be removed.",
        );
    }
    if let Some(deadline) = &suppression.review_after {
        if !valid_date(deadline) {
            return Err(format!(
                "invalid suppression review date at {}: expected YYYY-MM-DD",
                suppression.location.path.display()
            )
            .into());
        }
        if input
            .review_date
            .as_ref()
            .is_some_and(|today| deadline < today)
        {
            emit(
                report,
                AuditKind::SuppressionReviewOverdue,
                &suppression.location,
                Vec::new(),
                "The configured review deadline has passed; review supported profiles and rationale, not automatic suppression deletion.",
            );
        }
    }
    let mut related = Vec::new();
    for declaration in matches {
        let mut sources = source_locations(declaration);
        if !sources.is_empty() {
            sources.push(declaration.location.clone());
            related.extend(sources);
        }
    }
    if !related.is_empty() {
        emit(
            report,
            AuditKind::IgnoredButUsed,
            &suppression.location,
            related,
            "Unsuppressed source evidence references matching declarations. The suppression may still protect other profiles; this is not an automatic removal recommendation.",
        );
    }
    Ok(())
}

fn same_dependency(left: &AuditDeclaration, right: &AuditDeclaration) -> bool {
    left.location.dependency == right.location.dependency
        && left.dependency_package == right.dependency_package
        && left.specification.is_some()
        && left.specification == right.specification
}

fn audit_declaration(input: &AuditInput, declaration: &AuditDeclaration, report: &mut AuditReport) {
    if declaration.location.kind == Some(DependencyKind::Normal)
        && !declaration.optional
        && declaration.location.target.is_none()
        && declaration.usage.targets_complete
        && declaration.usage.production.is_empty()
        && declaration.usage.build.is_empty()
        && !declaration.usage.development.is_empty()
    {
        emit(
            report,
            AuditKind::NormalToDev,
            &declaration.location,
            source_locations(declaration),
            "Observed references are development-only with reliable target classification. Consider dev-dependencies only after reviewing supported profiles, feature forwarding, public API, doctests, and generated code; no global compiler-resolution proof is claimed.",
        );
    }
    if declaration.location.kind == Some(DependencyKind::Dev) && !declaration.optional {
        for normal in &input.declarations {
            if normal.location.path == declaration.location.path
                && normal.location.kind == Some(DependencyKind::Normal)
                && normal.location.target.is_none()
                && !normal.optional
                && same_dependency(normal, declaration)
            {
                emit(
                    report,
                    AuditKind::RedundantOccurrence,
                    &declaration.location,
                    vec![normal.location.clone()],
                    "An unconditional nonoptional normal declaration has the identical registry requirement and effective features. Review target/feature semantics and future separation before removing the dev occurrence.",
                );
            }
        }
    }
}

fn audit_workspace(input: &AuditInput, report: &mut AuditReport) {
    for workspace in &input.workspace_dependencies {
        let has_consumer = input.declarations.iter().any(|item| {
            item.workspace && item.location.dependency == workspace.location.dependency
        });
        if report.reference_inventory_complete && !has_consumer {
            emit(
                report,
                AuditKind::UnreferencedWorkspaceDeclaration,
                &workspace.location,
                Vec::new(),
                "No workspace-inherited occurrence references this declaration in the complete inventory, including unselected consumers. Review external tooling before changing it.",
            );
        }
        for item in input
            .declarations
            .iter()
            .filter(|item| item.selected && !item.workspace)
        {
            if item.location.dependency == workspace.location.dependency
                && item.dependency_package == workspace.dependency_package
                && item.specification.is_some()
                && workspace.specification.is_some()
                && item.specification != workspace.specification
            {
                emit(
                    report,
                    AuditKind::WorkspaceInconsistency,
                    &item.location,
                    vec![workspace.location.clone()],
                    "Direct registry configuration differs from the same-name workspace declaration. This may be intentional; feature/default-feature differences and version requirements are not automatically compatible.",
                );
            }
        }
    }
    for item in input.declarations.iter().filter(|item| {
        item.selected
            && !item.workspace
            && !item.optional
            && item.location.target.is_none()
            && item.specification.is_some()
    }) {
        if input
            .workspace_dependencies
            .iter()
            .any(|entry| entry.location.dependency == item.location.dependency)
        {
            continue;
        }
        let peers: Vec<_> = input
            .declarations
            .iter()
            .filter(|other| {
                !other.workspace
                    && !other.optional
                    && other.location.target.is_none()
                    && same_dependency(item, other)
            })
            .collect();
        let manifests: BTreeSet<_> = peers.iter().map(|peer| &peer.location.path).collect();
        if manifests.len() > 1
            && peers
                .iter()
                .all(|peer| item.location <= peer.location || !peer.selected)
        {
            emit(
                report,
                AuditKind::WorkspaceConsolidation,
                &item.location,
                peers
                    .iter()
                    .filter(|peer| peer.location != item.location)
                    .map(|peer| peer.location.clone())
                    .collect(),
                "Multiple packages repeat the identical registry requirement and effective features. Consider workspace inheritance after reviewing feature unification, publication, and independently supported versions; no rewrite is prescribed.",
            );
        }
    }
}
