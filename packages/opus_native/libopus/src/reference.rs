//! Build-bound provenance for the test-only public C reference.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

/// Returns the manifest embedded by this exact Cargo build.
///
/// Hashes identify this run's artifacts; they are not portable expected hashes.
/// # Panics
/// * Panics only if the build script embedded malformed JSON.
#[must_use]
pub fn reference_build() -> Value {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/reference.json")))
        .expect("build manifest")
}

/// Revalidates this build's source, effective configuration and artifact bytes.
///
/// # Errors
/// * Returns an error if files/git are unavailable, source changed, or artifacts/configuration differ.
pub fn verify_reference_build() -> Result<Value, String> {
    let manifest = reference_build();
    let text = |key| {
        manifest[key]
            .as_str()
            .ok_or_else(|| format!("missing {key}"))
    };
    let source = text("source")?;
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(source)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("cannot inspect reference revision".into());
    }
    let revision = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let clean = Command::new("git")
        .args(["diff", "--quiet", "HEAD", "--", "."])
        .current_dir(source)
        .status()
        .map_err(|e| e.to_string())?
        .success();
    crate::qualification::qualify(
        revision.trim(),
        clean,
        text("effective_config")?,
        text("cmake_cache")?,
    )?;
    for (path, expected) in [
        (
            std::path::PathBuf::from(text("archive")?),
            text("archive_sha256")?,
        ),
        (
            std::path::Path::new(text("build")?).join("config.h"),
            text("config_sha256")?,
        ),
        (
            std::path::Path::new(text("build")?).join("compile_commands.json"),
            text("compile_commands_sha256")?,
        ),
        (
            std::path::Path::new(text("build")?).join("CMakeCache.txt"),
            text("cache_sha256")?,
        ),
    ] {
        let actual = Sha256::digest(fs::read(path).map_err(|e| e.to_string())?)
            .iter()
            .fold(String::new(), |mut text, b| {
                use std::fmt::Write as _;
                write!(text, "{b:02x}").expect("writing string");
                text
            });
        if actual != expected {
            return Err("reference artifact/config changed since compilation".into());
        }
    }
    Ok(manifest)
}
