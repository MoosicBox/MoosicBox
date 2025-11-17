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

/// Result of a dry-run transform showing what would change
#[derive(Debug, Clone)]
pub struct TransformReport {
    /// Entries that would be removed
    pub would_remove: Vec<serde_json::Map<String, serde_json::Value>>,
    /// Entries that would be modified (before, after)
    pub would_modify: Vec<(
        serde_json::Map<String, serde_json::Value>,
        serde_json::Map<String, serde_json::Value>,
    )>,
    /// Entries that would remain unchanged
    pub unchanged: Vec<serde_json::Map<String, serde_json::Value>>,
    /// Total entries before
    pub before_count: usize,
    /// Total entries after
    pub after_count: usize,
}

impl TransformReport {
    /// Generate a human-readable summary
    #[must_use]
    pub fn summary(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        write!(
            &mut s,
            "Transform Report:\n  Before: {} entries\n  After: {} entries\n",
            self.before_count, self.after_count
        )
        .unwrap();
        writeln!(&mut s, "  Removed: {} entries", self.would_remove.len()).unwrap();
        writeln!(&mut s, "  Modified: {} entries", self.would_modify.len()).unwrap();
        writeln!(&mut s, "  Unchanged: {} entries", self.unchanged.len()).unwrap();
        s
    }
}

/// Apply transforms to a matrix with optional trace mode
///
/// # Errors
///
/// * Transform script fails to compile
/// * Transform script encounters runtime error
/// * Invalid transform specification
pub fn apply_transforms_with_trace(
    matrix: &mut Vec<serde_json::Map<String, serde_json::Value>>,
    transform_specs: &[String],
    workspace_root: &Path,
    trace_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if transform_specs.is_empty() {
        return Ok(());
    }

    let engine = TransformEngine::with_trace(workspace_root, trace_mode)?;

    for spec in transform_specs {
        let script = load_transform_script(spec, workspace_root)?;
        engine.apply_transform(matrix, &script)?;
    }

    Ok(())
}

/// Dry-run transforms to see what would change without modifying the matrix
///
/// # Errors
///
/// * Transform script fails to compile
/// * Transform script encounters runtime error
/// * Invalid transform specification
pub fn dry_run_transforms(
    matrix: &[serde_json::Map<String, serde_json::Value>],
    transform_specs: &[String],
    workspace_root: &Path,
) -> Result<TransformReport, Box<dyn std::error::Error>> {
    let before = matrix.to_vec();
    let mut after = matrix.to_vec();

    apply_transforms(&mut after, transform_specs, workspace_root)?;

    // Find differences
    let mut would_remove = Vec::new();
    let mut would_modify = Vec::new();
    let mut unchanged = Vec::new();

    for before_entry in &before {
        if let Some(after_entry) = after.iter().find(|e| entries_match_key(before_entry, e)) {
            if before_entry == after_entry {
                unchanged.push(before_entry.clone());
            } else {
                would_modify.push((before_entry.clone(), after_entry.clone()));
            }
        } else {
            would_remove.push(before_entry.clone());
        }
    }

    Ok(TransformReport {
        would_remove,
        would_modify,
        unchanged,
        before_count: before.len(),
        after_count: after.len(),
    })
}

/// Check if two matrix entries represent the same logical entry
fn entries_match_key(
    a: &serde_json::Map<String, serde_json::Value>,
    b: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    a.get("package") == b.get("package") && a.get("os") == b.get("os")
}

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
