//! Registry-backed comparison of exact published package artifacts.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read as _,
    path::Path,
    process::Command,
};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};

use super::{BoxError, PublishConfig, PublishPackageInfo, SanitizedPackage};
use crate::preferences::PurityPolicy;

#[cfg(test)]
mod tests;

const MAX_DOWNLOAD: u64 = 64 * 1024 * 1024;
const MAX_UNPACKED: u64 = 256 * 1024 * 1024;
const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;

/// Whether a published artifact matches the sanitized local package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurityStatus {
    /// All meaningful packaged files other than the root Cargo.lock match.
    Pure,
    /// Packaged files were added, removed, or changed.
    Impure,
    /// The comparison could not be completed safely.
    Unknown,
}

/// Artifact comparison, independent of publication status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurityAssessment {
    /// Package name.
    pub name: String,
    /// Exact published version being compared.
    pub version: String,
    /// Comparison outcome.
    pub status: PurityStatus,
    /// Pending selected packages transitively consuming this package.
    pub affected_pending_consumers: Vec<String>,
    /// Files present only locally, excluding the root Cargo.lock.
    pub added: Vec<String>,
    /// Files absent locally, excluding the root Cargo.lock.
    pub removed: Vec<String>,
    /// Files with different normalized contents, excluding the root Cargo.lock.
    pub changed: Vec<String>,
    /// Whether the root packaged Cargo.lock differs (including presence/absence).
    /// Reported separately: it does not determine dependency source purity, but
    /// can affect binary installation with `cargo install --locked`.
    #[serde(default)]
    pub lockfile_changed: bool,
    /// Failure details when comparison is unknown.
    pub diagnostics: Vec<String>,
}

/// Version normalization is a caller policy: publish must preserve exact versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestVersionPolicy {
    Preserve,
    // Release planning compares package contents across versions.
    #[allow(dead_code)]
    IgnorePackageVersion,
}

/// Read a bounded archive without extracting it. Reject ambiguous or unsafe paths,
/// links, duplicate files, and multiple package roots instead of hiding differences.
///
/// # Errors
/// * Invalid, unsafe, oversized, or unreadable archives and manifests.
pub fn normalized_archive_entries(
    path: &Path,
    policy: ManifestVersionPolicy,
) -> Result<BTreeMap<String, Vec<u8>>, BoxError> {
    if fs::metadata(path)?.len() > MAX_DOWNLOAD {
        return Err("Package archive exceeds compressed size limit".into());
    }
    let decoder = GzDecoder::new(fs::File::open(path)?).take(MAX_UNPACKED + 1);
    let mut archive = tar::Archive::new(decoder);
    let mut files = BTreeMap::new();
    let mut root: Option<String> = None;
    let mut seen_paths = BTreeSet::new();
    let mut total = 0u64;
    let mut count = 0usize;
    for entry in archive.entries()? {
        let mut entry = entry?;
        count += 1;
        if count > MAX_ENTRIES {
            return Err("Package archive has too many entries".into());
        }
        let path = std::str::from_utf8(entry.path_bytes().as_ref())?.to_string();
        if path.contains('\\') || path.starts_with('/') || path.contains(':') {
            return Err(format!("Unsafe archive path: {path}").into());
        }
        let components = path.trim_end_matches('/').split('/').collect::<Vec<_>>();
        if components
            .iter()
            .any(|part| part.is_empty() || *part == "." || *part == "..")
        {
            return Err(format!("Unsafe archive path: {path}").into());
        }
        let package_root = components.first().ok_or("Missing archive root")?;
        if root.as_deref().is_some_and(|root| root != *package_root) {
            return Err("Package archive contains multiple roots".into());
        }
        root = Some((*package_root).to_string());
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            continue;
        }
        if !kind.is_file() || components.len() < 2 {
            return Err(format!("Unsupported archive entry: {path}").into());
        }
        let relative = components[1..].join("/");
        if !seen_paths.insert(relative.clone()) {
            return Err(format!("Duplicate archive path: {relative}").into());
        }
        let size = entry.size();
        total = total.checked_add(size).ok_or("Archive size overflow")?;
        if size > MAX_FILE || total > MAX_UNPACKED {
            return Err("Package archive exceeds decompressed size limit".into());
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        if relative == ".cargo_vcs_info.json" || relative == "Cargo.toml.orig" {
            continue;
        }
        if relative == "Cargo.toml" {
            let mut manifest: toml::Value = toml::from_str(std::str::from_utf8(&bytes)?)?;
            if policy == ManifestVersionPolicy::IgnorePackageVersion
                && let Some(package) = manifest
                    .get_mut("package")
                    .and_then(toml::Value::as_table_mut)
            {
                package.remove("version");
            }
            bytes = serde_json::to_vec(&manifest)?;
        }
        if files.insert(relative.clone(), bytes).is_some() {
            return Err(format!("Duplicate archive path: {relative}").into());
        }
    }
    let mut decoder = archive.into_inner();
    std::io::copy(&mut decoder, &mut std::io::sink())?;
    if decoder.limit() == 0 {
        return Err("Package archive exceeds decompressed size limit".into());
    }
    if !files.contains_key("Cargo.toml") {
        return Err("Package archive has no Cargo.toml".into());
    }
    Ok(files)
}

pub(super) fn compare(
    package: &PublishPackageInfo,
    local: &BTreeMap<String, Vec<u8>>,
    published: &BTreeMap<String, Vec<u8>>,
    consumers: Vec<String>,
) -> PurityAssessment {
    let added = local
        .keys()
        .filter(|key| key.as_str() != "Cargo.lock" && !published.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let removed = published
        .keys()
        .filter(|key| key.as_str() != "Cargo.lock" && !local.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    let changed = local
        .iter()
        .filter(|(key, value)| {
            key.as_str() != "Cargo.lock" && published.get(*key).is_some_and(|old| old != *value)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    PurityAssessment {
        name: package.name.clone(),
        version: package.version.clone(),
        status: if added.is_empty() && removed.is_empty() && changed.is_empty() {
            PurityStatus::Pure
        } else {
            PurityStatus::Impure
        },
        affected_pending_consumers: consumers,
        added,
        removed,
        changed,
        lockfile_changed: local.get("Cargo.lock") != published.get("Cargo.lock"),
        diagnostics: Vec::new(),
    }
}

fn consumers(
    packages: &BTreeMap<String, PublishPackageInfo>,
    pending: &BTreeSet<String>,
    dependency: &str,
) -> Vec<String> {
    pending
        .iter()
        .filter(|name| {
            let mut stack = vec![name.as_str()];
            let mut seen = BTreeSet::new();
            while let Some(name) = stack.pop() {
                if !seen.insert(name) {
                    continue;
                }
                if name == dependency {
                    return true;
                }
                if let Some(package) = packages.get(name) {
                    stack.extend(package.publish_dependencies.iter().map(String::as_str));
                }
            }
            false
        })
        .cloned()
        .collect()
}

pub(super) async fn artifact_comparison(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package: &PublishPackageInfo,
    config: &PublishConfig,
    client: &super::RegistryClient,
    download_url: &str,
) -> Result<(BTreeMap<String, Vec<u8>>, BTreeMap<String, Vec<u8>>), BoxError> {
    super::preflight::audit_cargo_configuration(workspace_root)?;
    let staged = SanitizedPackage::prepare(workspace_root, packages, package)?;
    // Cargo includes the workspace lockfile in package artifacts when applicable.
    if workspace_root.join("Cargo.lock").is_file() {
        fs::copy(
            workspace_root.join("Cargo.lock"),
            staged.package_path.join("Cargo.lock"),
        )?;
    }
    let output = Command::new("cargo")
        .arg("package")
        .arg("--no-verify")
        .arg("--registry")
        .arg("crates-io")
        .arg("--manifest-path")
        .arg(staged.manifest_path())
        .arg("--color")
        .arg(super::cargo_color_arg(config.color))
        .arg("--allow-dirty")
        .current_dir(staged.root())
        .env("CARGO_TARGET_DIR", staged.target_dir())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Cannot package local artifact for purity comparison: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let local = staged
        .target_dir()
        .join("package")
        .join(format!("{}-{}.crate", package.name, package.version));
    let local = normalized_archive_entries(&local, ManifestVersionPolicy::Preserve)?;
    let downloaded = staged.root().join("published.crate");
    download_artifact(client, download_url, &downloaded, MAX_DOWNLOAD).await?;
    let published = normalized_archive_entries(&downloaded, ManifestVersionPolicy::Preserve)?;
    Ok((local, published))
}

/// Stream with both declared and observed length bounds. Keep the bound injectable
/// so tests exercise the production checks without large allocations.
async fn download_artifact(
    client: &super::RegistryClient,
    download_url: &str,
    destination: &Path,
    max_download: u64,
) -> Result<(), BoxError> {
    let mut response = client.get(download_url).await?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > max_download)
    {
        return Err("Registry artifact exceeds download limit".into());
    }
    let mut file = fs::File::create(destination)?;
    let mut downloaded_size = 0u64;
    while let Some(chunk) = response.chunk().await? {
        downloaded_size = downloaded_size
            .checked_add(u64::try_from(chunk.len())?)
            .ok_or("Registry artifact download size overflow")?;
        if downloaded_size > max_download {
            return Err("Registry artifact exceeds download limit".into());
        }
        std::io::Write::write_all(&mut file, &chunk)?;
    }
    Ok(())
}

pub(super) async fn assess(
    workspace_root: &Path,
    packages: &BTreeMap<String, PublishPackageInfo>,
    package: &PublishPackageInfo,
    pending: &BTreeSet<String>,
    config: &PublishConfig,
    client: &super::RegistryClient,
) -> PurityAssessment {
    let consumers = consumers(packages, pending, &package.name);
    let url = format!(
        "https://static.crates.io/crates/{0}/{0}-{1}.crate",
        package.name, package.version
    );
    match artifact_comparison(workspace_root, packages, package, config, client, &url).await {
        Ok((local, published)) => compare(package, &local, &published, consumers),
        Err(error) => PurityAssessment {
            name: package.name.clone(),
            version: package.version.clone(),
            status: PurityStatus::Unknown,
            affected_pending_consumers: consumers,
            added: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
            lockfile_changed: false,
            diagnostics: vec![error.to_string()],
        },
    }
}

pub(super) fn enforce(
    policy: PurityPolicy,
    assessments: &[PurityAssessment],
) -> Result<(), BoxError> {
    if policy == PurityPolicy::RequirePure
        && assessments
            .iter()
            .any(|assessment| assessment.status != PurityStatus::Pure)
    {
        return Err(Box::new(PublishPurityError {
            assessments: assessments.to_vec(),
        }));
    }
    Ok(())
}

/// Strict purity failure. Full structured diagnostics remain available to library callers.
/// Display is deliberately bounded for terminal use.
#[derive(Debug, Clone, Serialize)]
pub struct PublishPurityError {
    /// All assessments collected before stopping, including unchanged packages.
    pub assessments: Vec<PurityAssessment>,
}

impl std::fmt::Display for PublishPurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let failures = self
            .assessments
            .iter()
            .filter(|assessment| assessment.status != PurityStatus::Pure)
            .count();
        write!(
            f,
            "Publish purity required: {failures} impure/unknown package(s); blocking all subsequent uploads (including dry-run). Earlier uploads cannot be rolled back. Bump the dependency version and update dependent requirements, or restore the published contents; resolve unknown comparison diagnostics before retrying."
        )
    }
}

impl std::error::Error for PublishPurityError {}

fn bounded_text(text: &str) -> String {
    let mut result = text
        .chars()
        .take(512)
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    if text.chars().count() > 512 {
        result.push('…');
    }
    result
}

fn file_changes(assessment: &PurityAssessment) -> String {
    [
        (assessment.changed.len(), "modified"),
        (assessment.added.len(), "added"),
        (assessment.removed.len(), "removed"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, label)| format!("{count} {label}"))
    .collect::<Vec<_>>()
    .join(", ")
}

pub(super) fn summary(assessment: &PurityAssessment) -> String {
    let package = format!(
        "{}@{}",
        bounded_text(&assessment.name),
        bounded_text(&assessment.version)
    );
    match assessment.status {
        PurityStatus::Pure if assessment.lockfile_changed => format!(
            "{package}: source/manifest matches crates.io; packaged Cargo.lock differs (relevant to cargo install --locked)"
        ),
        PurityStatus::Pure => format!("{package}: matches crates.io"),
        PurityStatus::Impure => format!(
            "{package}: differs from crates.io ({})",
            file_changes(assessment)
        ),
        PurityStatus::Unknown => format!("{package}: could not verify published contents"),
    }
}

fn preview(values: &[String]) -> String {
    let text = values
        .iter()
        .take(3)
        .map(|value| bounded_text(value))
        .collect::<Vec<_>>()
        .join(", ");
    if values.len() > 3 {
        format!("{text} (+{} more)", values.len() - 3)
    } else {
        text
    }
}

fn diagnostic(
    policy: PurityPolicy,
    assessment: &PurityAssessment,
    color: crate::ColorMode,
) -> String {
    use super::{format_package_name, paint};
    let blocking = policy == PurityPolicy::RequirePure;
    let (severity, code) = if blocking {
        ("error", "31;1")
    } else {
        ("warning", "33;1")
    };
    let package = format_package_name(
        color,
        &bounded_text(&assessment.name),
        &bounded_text(&assessment.version),
    );
    let explanation = match assessment.status {
        PurityStatus::Impure => "local contents differ from the published version",
        PurityStatus::Unknown => "could not verify the published contents",
        PurityStatus::Pure if assessment.lockfile_changed => {
            return format!(
                "{}: {package} source/manifest matches crates.io; packaged Cargo.lock differs.\n  Dependency consumers resolve with their own lockfile; cargo install --locked uses the published lockfile.",
                paint(color, "34", "note")
            );
        }
        PurityStatus::Pure => return String::new(),
    };
    let mut lines = vec![format!(
        "{}: {package} {explanation}",
        paint(color, code, severity)
    )];
    if assessment.status == PurityStatus::Impure {
        lines
            .push("  Consumers will receive the crates.io version, not your local changes.".into());
        for (label, files) in [
            ("Modified", &assessment.changed),
            ("Added", &assessment.added),
            ("Removed", &assessment.removed),
        ] {
            if !files.is_empty() {
                lines.push(format!("  {label}: {}", preview(files)));
            }
        }
        lines.push("  Publish a new version and update dependent requirements, or restore the published contents.".into());
    } else {
        if let Some(reason) = assessment.diagnostics.first() {
            lines.push(format!("  Reason: {}", bounded_text(reason)));
        }
        lines.push(
            "  Resolve the comparison failure and retry; this package has not been confirmed pure."
                .into(),
        );
    }
    if assessment.lockfile_changed {
        lines.push("  Packaged Cargo.lock also differs (reported separately; relevant to cargo install --locked).".into());
    }
    if !assessment.affected_pending_consumers.is_empty() {
        lines.push(format!(
            "  Affected packages: {}",
            preview(&assessment.affected_pending_consumers)
        ));
    }
    let action = if blocking {
        "Publishing blocked: require-pure is enabled."
    } else {
        "Continuing. Use --require-pure to block publishing when purity cannot be established."
    };
    lines.push(format!("  {}", paint(color, code, action)));
    lines.join("\n")
}

pub(super) fn warn(policy: PurityPolicy, assessment: &PurityAssessment, color: crate::ColorMode) {
    use std::io::IsTerminal as _;
    let color = match color {
        crate::ColorMode::Auto if !std::io::stderr().is_terminal() => crate::ColorMode::Never,
        color => color,
    };
    let text = diagnostic(policy, assessment, color);
    if !text.is_empty() {
        eprintln!("{text}");
    }
}
