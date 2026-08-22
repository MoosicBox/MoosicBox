//! Checked Cargo execution and exact compiler-artifact resolution.

use std::{fs, io::Cursor, process::Command};

use anyhow::{Context, Result, bail};
use cargo_metadata::{Message, Metadata, camino::Utf8PathBuf};

use crate::{ArtifactKind, Scenario, TargetSelection};

/// Exact final artifact emitted by Cargo.
pub(crate) struct BuiltArtifact {
    pub(crate) path: Utf8PathBuf,
    pub(crate) size: u64,
    pub(crate) fresh: bool,
}

/// Builds one scenario and resolves the exact final artifact from Cargo JSON messages.
///
/// # Errors
///
/// * Cargo cannot be started, exits unsuccessfully, emits unreadable messages, or does not emit
///   the selected final artifact
pub(crate) fn build_scenario(
    metadata: &Metadata,
    target: &TargetSelection,
    profile: &str,
    compilation_target: Option<&str>,
    scenario: &Scenario,
) -> Result<BuiltArtifact> {
    let mut command = build_command(metadata, target, profile, compilation_target, scenario);
    let printable = format!("{command:?}");
    eprintln!("$ {printable}");
    let output = command.output().context("failed to start Cargo build")?;
    let messages = Message::parse_stream(Cursor::new(&output.stdout))
        .collect::<std::io::Result<Vec<_>>>()
        .context("failed to parse Cargo compiler messages")?;

    for message in &messages {
        if let Message::CompilerMessage(message) = message
            && message.package_id == target.package_id
            && let Some(rendered) = &message.message.rendered
        {
            eprint!("{rendered}");
        }
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Cargo build failed with status {}: {printable}\n{}",
            output.status,
            stderr.trim()
        );
    }

    resolve_artifact(target, messages)
}

fn build_command(
    metadata: &Metadata,
    target: &TargetSelection,
    profile: &str,
    compilation_target: Option<&str>,
    scenario: &Scenario,
) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(metadata.workspace_root.as_std_path())
        .arg("build")
        .arg("--message-format=json-render-diagnostics")
        .arg("--package")
        .arg(&target.package_name)
        .arg(target.kind.cargo_flag())
        .arg(&target.target_name)
        .arg("--profile")
        .arg(profile);
    if !scenario.config.default_features {
        command.arg("--no-default-features");
    }
    if !scenario.config.features.is_empty() {
        command.arg("--features").arg(
            scenario
                .config
                .features
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if let Some(compilation_target) = compilation_target {
        command.arg("--target").arg(compilation_target);
    }
    command
}

fn resolve_artifact(target: &TargetSelection, messages: Vec<Message>) -> Result<BuiltArtifact> {
    let artifact = messages
        .into_iter()
        .rev()
        .find_map(|message| match message {
            Message::CompilerArtifact(artifact)
                if artifact.package_id == target.package_id
                    && artifact.target.name == target.target_name =>
            {
                Some(artifact)
            }
            _ => None,
        });
    let artifact = artifact.with_context(|| {
        format!(
            "Cargo did not emit an artifact for package '{}' target '{}'",
            target.package_name, target.target_name
        )
    })?;

    let path = match target.kind {
        ArtifactKind::Binary => artifact.executable,
        ArtifactKind::Cdylib | ArtifactKind::Dylib | ArtifactKind::Staticlib => artifact
            .filenames
            .into_iter()
            .find(|path| is_library_artifact(path, target.kind)),
    }
    .with_context(|| {
        format!(
            "Cargo artifact for target '{}' did not contain a measurable final file",
            target.target_name
        )
    })?;
    let size = fs::metadata(&path)
        .with_context(|| format!("failed to read artifact metadata for {path}"))?
        .len();
    Ok(BuiltArtifact {
        path,
        size,
        fresh: artifact.fresh,
    })
}

fn is_library_artifact(path: &Utf8PathBuf, kind: ArtifactKind) -> bool {
    let extension = path.extension().unwrap_or_default();
    match kind {
        ArtifactKind::Binary => false,
        ArtifactKind::Cdylib | ArtifactKind::Dylib => matches!(extension, "so" | "dylib" | "dll"),
        ArtifactKind::Staticlib => matches!(extension, "a" | "lib"),
    }
}

#[cfg(test)]
mod tests {
    use cargo_metadata::PackageId;

    use super::*;
    use crate::{FeatureConfig, TargetSelection};

    #[test]
    fn resolves_the_exact_compiler_artifact() {
        let directory = tempfile::tempdir().unwrap();
        let artifact_path = directory.path().join("example-bin");
        fs::write(&artifact_path, b"exact artifact").unwrap();
        let package_id = PackageId {
            repr: "path+file:///tmp/example#0.1.0".to_owned(),
        };
        let message = serde_json::json!({
            "reason": "compiler-artifact",
            "package_id": package_id.repr,
            "manifest_path": "/tmp/example/Cargo.toml",
            "target": {
                "kind": ["bin"],
                "crate_types": ["bin"],
                "name": "example-bin",
                "src_path": "/tmp/example/src/main.rs",
                "edition": "2024",
                "doc": true,
                "doctest": false,
                "test": true
            },
            "profile": {
                "opt_level": "0",
                "debuginfo": 2,
                "debug_assertions": true,
                "overflow_checks": true,
                "test": false
            },
            "features": [],
            "filenames": [artifact_path],
            "executable": artifact_path,
            "fresh": false
        });
        let message: Message = serde_json::from_value(message).unwrap();
        let target = TargetSelection {
            package_id,
            package_name: "example".to_owned(),
            target_name: "example-bin".to_owned(),
            kind: ArtifactKind::Binary,
            required_features: Vec::new(),
            available_features: std::collections::BTreeSet::default(),
        };

        let artifact = resolve_artifact(&target, vec![message]).unwrap();
        assert_eq!(artifact.size, 14);
        assert_eq!(artifact.path.as_std_path(), artifact_path);
    }

    #[test]
    fn command_contains_explicit_build_dimensions() {
        let mut command = Command::new("cargo");
        command.args(["metadata", "--no-deps", "--format-version", "1"]);
        let output = command.output().unwrap();
        let metadata: Metadata = serde_json::from_slice(&output.stdout).unwrap();
        let target = TargetSelection {
            package_id: PackageId {
                repr: "path+file:///tmp/example#0.1.0".to_owned(),
            },
            package_name: "example".to_owned(),
            target_name: "example-bin".to_owned(),
            kind: ArtifactKind::Binary,
            required_features: Vec::new(),
            available_features: std::collections::BTreeSet::default(),
        };
        let scenario = Scenario {
            name: "test".to_owned(),
            config: FeatureConfig {
                default_features: false,
                features: ["alpha".to_owned(), "beta".to_owned()].into(),
            },
        };
        let command = build_command(
            &metadata,
            &target,
            "small",
            Some("aarch64-apple-darwin"),
            &scenario,
        );
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|args| args == ["--package", "example"]));
        assert!(args.windows(2).any(|args| args == ["--bin", "example-bin"]));
        assert!(args.windows(2).any(|args| args == ["--profile", "small"]));
        assert!(args.contains(&"--no-default-features".to_owned()));
        assert!(args.contains(&"alpha,beta".to_owned()));
    }
}
