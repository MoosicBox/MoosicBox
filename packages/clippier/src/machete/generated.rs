//! Explicit, bounded generated-source evidence. This module never runs a compiler.
//!
//! Exact input snapshots establish freshness of the *declared* inputs only. They
//! cannot establish that a producer declared every input, or prove absence of a
//! dependency in other targets, profiles, expansions or compiler versions.
use super::{BoxError, source};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;

/// Exact-byte fingerprint, deliberately collision-free rather than a short hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFingerprint {
    /// Root-relative path; symlinks, absolute paths and parent traversal are forbidden.
    pub path: PathBuf,
    /// Expected current bytes. Producers must capture inputs when generating output.
    pub bytes: Vec<u8>,
}

impl InputFingerprint {
    /// Capture an existing regular file without executing its producer.
    ///
    /// # Errors
    /// * Invalid, escaping or symlink paths, oversized files and filesystem errors.
    pub fn capture(root: &Path, path: &Path) -> Result<Self, BoxError> {
        let absolute = confined_file(root, path)?;
        Ok(Self {
            path: path.to_owned(),
            bytes: bounded_read(&absolute)?,
        })
    }
}

/// Cargo target to which supplied text belongs, not a universal coverage claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeneratedTargetRole {
    /// Library source.
    Library,
    /// Named binary.
    Binary(String),
    /// Named integration test.
    Test(String),
    /// Named example.
    Example(String),
    /// Named benchmark.
    Bench(String),
    /// Build-script source.
    BuildScript,
}

/// One externally supplied expanded/generated Rust file and its bounded provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedSource {
    /// Root-relative manifest owning this evidence.
    pub owner_manifest: PathBuf,
    /// Exact target role.
    pub target_role: GeneratedTargetRole,
    /// Producer-defined configuration label (features, cfgs and compiler identity).
    pub profile: String,
    /// Output file path and expected bytes, captured at generation time.
    pub output: InputFingerprint,
    /// Declared current producer inputs; must include the owning manifest.
    pub inputs: Vec<InputFingerprint>,
}

/// Explicit supplied evidence; absence of sources supplies no additional coverage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedEvidence {
    /// Generated/expanded sources, never automatically executed or refreshed.
    pub sources: Vec<GeneratedSource>,
}

/// Validated lexical evidence for one target/profile only.
#[derive(Debug, Clone, Serialize)]
pub struct GeneratedSourceReport {
    /// Owning manifest relative to the analysis root.
    pub owner_manifest: PathBuf,
    /// Target scope copied from the producer.
    pub target_role: GeneratedTargetRole,
    /// Configuration scope copied from the producer.
    pub profile: String,
    /// Validated generated file.
    pub path: PathBuf,
    /// Observed lexical dependency roots, not compiler name resolution.
    pub names: BTreeSet<String>,
    /// Lexical limitations and unresolved constructs.
    pub diagnostics: BTreeSet<String>,
}

/// Supplied evidence is additive positive evidence, never global absence proof.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GeneratedEvidenceReport {
    /// Independently scoped validated files.
    pub sources: Vec<GeneratedSourceReport>,
}

/// Injectable source supplier. Implementations must not silently execute producers.
pub trait GeneratedEvidenceCollector {
    /// Return explicit generated files and generation-time input snapshots.
    ///
    /// # Errors
    /// * Producer-specific collection or provenance failures.
    fn collect(&self, root: &Path) -> Result<GeneratedEvidence, BoxError>;
}

/// Collect and validate supplied evidence without invoking Cargo or a compiler.
///
/// # Errors
/// * Collector errors and all freshness/path errors from [`validate_generated_evidence`].
pub fn collect_generated_evidence(
    root: &Path,
    collector: &dyn GeneratedEvidenceCollector,
) -> Result<GeneratedEvidenceReport, BoxError> {
    validate_generated_evidence(root, &collector.collect(root)?)
}

/// Validate exact current bytes and extract conservative lexical positive evidence.
/// No supplied source can mark other targets/profiles complete or authorize removal.
///
/// # Errors
/// * Empty profiles/inputs, missing owner snapshot, invalid manifests or UTF-8.
/// * Stale bytes, conflicting snapshots, symlinks, escaping/oversized files and I/O errors.
pub fn validate_generated_evidence(
    root: &Path,
    evidence: &GeneratedEvidence,
) -> Result<GeneratedEvidenceReport, BoxError> {
    let mut report = GeneratedEvidenceReport::default();
    for supplied in &evidence.sources {
        if supplied.profile.trim().is_empty() || supplied.inputs.is_empty() {
            return Err("generated evidence requires a profile and declared inputs".into());
        }
        if !supplied
            .inputs
            .iter()
            .any(|input| input.path == supplied.owner_manifest)
        {
            return Err("generated inputs must fingerprint the owning manifest".into());
        }
        let owner = confined_file(root, &supplied.owner_manifest)?;
        if owner.file_name().is_none_or(|name| name != "Cargo.toml") {
            return Err("generated evidence owner must be Cargo.toml".into());
        }
        let _: toml::Value = toml::from_str(&fs::read_to_string(owner)?)?;
        for fingerprint in supplied
            .inputs
            .iter()
            .chain(std::iter::once(&supplied.output))
        {
            if bounded_read(&confined_file(root, &fingerprint.path)?)? != fingerprint.bytes {
                return Err(
                    format!("stale generated evidence: {}", fingerprint.path.display()).into(),
                );
            }
        }
        let text = std::str::from_utf8(&supplied.output.bytes)?;
        let lexical = source::inspect(text);
        let diagnostics = lexical
            .incomplete
            .into_iter()
            .chain(lexical.advisory)
            .chain(lexical.uncertain)
            .collect();
        report.sources.push(GeneratedSourceReport {
            owner_manifest: supplied.owner_manifest.clone(),
            target_role: supplied.target_role.clone(),
            profile: supplied.profile.clone(),
            path: supplied.output.path.clone(),
            names: lexical.names,
            diagnostics,
        });
    }
    Ok(report)
}

fn bounded_read(path: &Path) -> Result<Vec<u8>, BoxError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err("generated input exceeds 16 MiB".into());
    }
    Ok(bytes)
}

fn confined_file(root: &Path, path: &Path) -> Result<PathBuf, BoxError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("generated paths must be normal root-relative paths".into());
    }
    for ancestor in root
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
    {
        if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
            return Err("generated evidence root traverses a symlink".into());
        }
    }
    let root = root.canonicalize()?;
    let mut current = root;
    for component in path.components() {
        current.push(component);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(
                format!("generated evidence rejects symlinks: {}", current.display()).into(),
            );
        }
    }
    if !fs::metadata(&current)?.is_file() {
        return Err("generated evidence requires regular files".into());
    }
    Ok(current)
}

/// Explicit expansion capability result; stable rustc has no supported expansion API.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpansionCapability {
    /// No execution took place. Supply externally generated, fingerprinted files.
    Unsupported {
        /// Reason and supported alternative.
        reason: String,
    },
}

/// Query the stable-toolchain adapter without invoking rustc, rustup or Cargo.
/// No nightly selection, installation, network access or implicit macro execution occurs.
#[must_use]
pub fn stable_expansion_capability() -> ExpansionCapability {
    ExpansionCapability::Unsupported { reason: "stable rustc has no supported full macro-expansion interface; supply explicit GeneratedEvidence from a trusted producer; compiler verification is a separate opt-in lane".into() }
}
