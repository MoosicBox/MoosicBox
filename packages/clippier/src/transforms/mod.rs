//! Matrix transformation system with Lua scripting support.
//!
//! This module provides a powerful transformation system that allows users to
//! write custom Lua scripts to transform the CI matrix with full access to
//! workspace metadata, dependency graphs, and package information.
//!
//! # Features
//!
//! * User-defined Lua scripts for matrix transformation
//! * Full workspace context with package metadata
//! * Dependency graph analysis
//! * Platform-specific feature detection
//! * Support for inline scripts, file references, and named transforms

mod context;
mod engine;

pub use context::{DependencyInfo, PackageInfo, TransformContext};
pub use engine::TransformEngine;

use std::path::Path;

/// Apply transforms to a matrix
///
/// # Errors
///
/// * Transform script fails to compile
/// * Transform script encounters runtime error
/// * Invalid transform specification
pub fn apply_transforms(
    matrix: &mut Vec<serde_json::Map<String, serde_json::Value>>,
    transform_specs: &[String],
    workspace_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if transform_specs.is_empty() {
        return Ok(());
    }

    let engine = TransformEngine::new(workspace_root)?;

    for spec in transform_specs {
        let script = load_transform_script(spec, workspace_root)?;
        engine.apply_transform(matrix, &script)?;
    }

    Ok(())
}

/// Load a transform script from various sources
///
/// Supports:
/// - Inline Lua code
/// - File paths (.lua extension)
/// - Named transforms from .clippier/clippier.toml
fn load_transform_script(
    spec: &str,
    workspace_root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let spec = spec.trim();

    // Check if it's a file path
    if std::path::Path::new(spec)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        let path = if spec.starts_with('.') {
            workspace_root.join(spec)
        } else {
            std::path::PathBuf::from(spec)
        };

        if path.exists() {
            return Ok(std::fs::read_to_string(path)?);
        }
    }

    // Check if it's a named transform from .clippier/clippier.toml
    let clippier_config = workspace_root.join(".clippier/clippier.toml");
    if clippier_config.exists() {
        let content = std::fs::read_to_string(clippier_config)?;
        let config: toml::Value = toml::from_str(&content)?;

        if let Some(transforms) = config.get("transforms").and_then(|t| t.as_array()) {
            for transform in transforms {
                if transform.get("name").and_then(|n| n.as_str()) == Some(spec) {
                    // Found named transform
                    if let Some(script) = transform.get("script").and_then(|s| s.as_str()) {
                        return Ok(script.to_string());
                    }
                    if let Some(file) = transform.get("script-file").and_then(|s| s.as_str()) {
                        let script_path = workspace_root.join(file);
                        return Ok(std::fs::read_to_string(script_path)?);
                    }
                }
            }
        }
    }

    // Treat as inline Lua script
    Ok(spec.to_string())
}
