//! Private `cargo-semver-checks` adapter.

use std::path::Path;

use cargo_semver_checks::{Check, GlobalConfig, ReleaseType, Rustdoc};
use serde::{Deserialize, Serialize};

use super::{EffectiveSemverConfig, SemverFeaturePolicy};

/// Clippier-owned compatibility classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Compatibility {
    /// The package has no known compatibility requirement beyond a patch release.
    Patch,
    /// The package requires a compatible feature release.
    Feature,
    /// The package requires an incompatible release.
    Breaking,
}

/// Result of checking one package against a published registry baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemverAnalysis {
    /// Cargo package name.
    pub package: String,
    /// Baseline registry version.
    pub baseline_version: String,
    /// Compatibility requirement found by `cargo-semver-checks`.
    pub compatibility: Compatibility,
    /// Deterministic upstream feature policy used by this analysis.
    pub feature_policy: String,
    /// Features enabled for both current and baseline sources.
    pub features: Vec<String>,
    /// Effective additional features enabled for current source.
    pub current_features: Vec<String>,
    /// Effective additional features enabled for baseline source.
    pub baseline_features: Vec<String>,
    /// Version of the private analysis engine.
    pub engine_version: String,
}

/// Check a package's current source against an exact crates.io baseline.
///
/// The adapter assumes a patch release so that `required_bump()` reports whether a compatible
/// feature or incompatible release is necessary. Upstream manifest lint configuration remains
/// authoritative because the current package is loaded from its real workspace manifest.
///
/// # Errors
///
/// * If current or baseline rustdoc generation fails
/// * If the package cannot be found in the current workspace or registry
/// * If `cargo-semver-checks` reports required witness execution errors
/// * If no crate report is produced for the selected package
pub(super) fn analyze(
    workspace_root: &Path,
    package: &str,
    baseline_version: &str,
    feature_config: &EffectiveSemverConfig,
) -> anyhow::Result<SemverAnalysis> {
    let mut check = Check::new(Rustdoc::from_root(workspace_root));
    check
        .set_packages(vec![package.to_string()])
        .set_baseline(Rustdoc::from_registry(baseline_version))
        .set_release_type(ReleaseType::Patch);
    match feature_config.feature_policy {
        SemverFeaturePolicy::DefaultHeuristic => {
            check.with_heuristically_included_features();
        }
        SemverFeaturePolicy::All => {
            check.with_all_features();
        }
        SemverFeaturePolicy::DefaultOnly => {
            check.with_default_features();
        }
        SemverFeaturePolicy::ExplicitOnly => {
            check.with_only_explicit_features();
        }
    }
    let mut current_features = feature_config.current_features.clone();
    current_features.extend(feature_config.features.iter().cloned());
    current_features.sort();
    current_features.dedup();
    let effective_current_features = current_features.clone();
    let mut baseline_features = feature_config.baseline_features.clone();
    baseline_features.extend(feature_config.features.iter().cloned());
    baseline_features.sort();
    baseline_features.dedup();
    let effective_baseline_features = baseline_features.clone();
    check.set_extra_features(current_features, baseline_features);

    let mut config = GlobalConfig::new();
    config
        .set_log_level(None)
        .set_color_choice(false)
        .set_stdout(Box::new(std::io::sink()))
        .set_stderr(Box::new(std::io::sink()));
    let report = check.check_release(&mut config)?;
    if report.has_required_witness_errors() {
        anyhow::bail!("cargo-semver-checks encountered required witness execution errors");
    }
    let crate_report = report
        .crate_reports()
        .get(package)
        .ok_or_else(|| anyhow::anyhow!("cargo-semver-checks produced no report for '{package}'"))?;
    let compatibility = match crate_report.required_bump() {
        Some(ReleaseType::Major) => Compatibility::Breaking,
        Some(ReleaseType::Minor) => Compatibility::Feature,
        Some(ReleaseType::Patch) | None => Compatibility::Patch,
        Some(_) => anyhow::bail!("cargo-semver-checks returned an unsupported release type"),
    };

    Ok(SemverAnalysis {
        package: package.to_string(),
        baseline_version: baseline_version.to_string(),
        compatibility,
        feature_policy: feature_config.feature_policy.as_str().to_string(),
        features: feature_config.features.clone(),
        current_features: effective_current_features,
        baseline_features: effective_baseline_features,
        engine_version: cargo_semver_checks_version(workspace_root)?,
    })
}

fn cargo_semver_checks_version(workspace_root: &Path) -> anyhow::Result<String> {
    let mut command = cargo_metadata::MetadataCommand::new();
    command.current_dir(workspace_root);
    let metadata = command.exec()?;
    metadata
        .packages
        .iter()
        .find(|package| package.name == "cargo-semver-checks")
        .map(|package| package.version.to_string())
        .ok_or_else(|| anyhow::anyhow!("cargo-semver-checks is missing from Cargo metadata"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_order_is_monotonic() {
        assert!(Compatibility::Patch < Compatibility::Feature);
        assert!(Compatibility::Feature < Compatibility::Breaking);
    }
}
