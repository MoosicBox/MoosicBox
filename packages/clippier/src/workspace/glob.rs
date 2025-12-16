//! Shared glob expansion utilities for workspace member discovery.
//!
//! This module provides async glob expansion that works for both Cargo and Node.js
//! workspace patterns.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{LazyLock, RwLock},
};

use globset::GlobMatcher;

/// Global cache for compiled glob patterns to avoid recompilation.
static GLOB_CACHE: LazyLock<RwLock<BTreeMap<String, GlobMatcher>>> =
    LazyLock::new(|| RwLock::new(BTreeMap::new()));

/// Returns a compiled glob matcher for the given pattern, using a cache.
///
/// # Arguments
///
/// * `pattern` - The glob pattern to compile (e.g., `packages/*`, `pkg_[abc]`)
///
/// # Returns
///
/// * `Some(GlobMatcher)` - A compiled matcher for pattern matching
/// * `None` - If the pattern is invalid and cannot be compiled
#[must_use]
pub fn get_or_compile_glob(pattern: &str) -> Option<GlobMatcher> {
    // Fast path: check if already cached (read lock)
    if let Some(matcher) = GLOB_CACHE.read().ok().and_then(|c| c.get(pattern).cloned()) {
        return Some(matcher);
    }

    // Slow path: compile and cache (write lock)
    let matcher = globset::Glob::new(pattern).ok()?.compile_matcher();
    if let Ok(mut cache) = GLOB_CACHE.write() {
        cache.insert(pattern.to_string(), matcher.clone());
    }
    Some(matcher)
}

/// Checks if a pattern contains glob special characters.
#[must_use]
pub fn contains_glob_chars(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

/// Expands workspace member glob patterns to actual directory paths.
///
/// This function handles patterns like:
/// - `packages/*` - All direct children of packages/
/// - `packages/**` - All descendants of packages/
/// - `pkg_[abc]` - Character class patterns
/// - `src/*/lib` - Nested patterns
///
/// # Arguments
///
/// * `root` - The workspace root directory
/// * `patterns` - Glob patterns to expand
/// * `manifest_file` - The manifest file to check for (e.g., `Cargo.toml`, `package.json`)
///
/// # Returns
///
/// A list of relative paths (from workspace root) to valid package directories.
pub async fn expand_workspace_globs(
    root: &Path,
    patterns: &[&str],
    manifest_file: &str,
) -> Vec<String> {
    let mut expanded = Vec::new();

    for pattern in patterns {
        // Handle negation patterns (pnpm uses !pattern for exclusions)
        if pattern.starts_with('!') {
            // TODO: Handle exclusions - for now we skip them
            log::trace!("Skipping exclusion pattern: {pattern}");
            continue;
        }

        if contains_glob_chars(pattern) {
            // Expand the glob
            let matches = expand_simple_glob_pattern(root, pattern, manifest_file).await;
            expanded.extend(matches);
        } else {
            // Direct path, just verify it exists
            let full_path = root.join(pattern);
            let manifest_path = full_path.join(manifest_file);
            if switchy_fs::unsync::exists(&manifest_path).await {
                expanded.push((*pattern).to_string());
            }
        }
    }

    expanded
}

/// Expands a single glob pattern to matching directories.
///
/// # Arguments
///
/// * `root` - The workspace root directory
/// * `pattern` - A single glob pattern (e.g., `packages/*`)
/// * `manifest_file` - The manifest file to check for
async fn expand_simple_glob_pattern(
    root: &Path,
    pattern: &str,
    manifest_file: &str,
) -> Vec<String> {
    let mut results = Vec::new();

    // For simple `foo/*` patterns, we can optimize by just listing the directory
    if pattern.ends_with("/*") && !pattern[..pattern.len() - 2].contains('*') {
        let base_dir = &pattern[..pattern.len() - 2];
        let full_base = root.join(base_dir);

        if let Ok(entries) = switchy_fs::unsync::read_dir_sorted(&full_base).await {
            for entry in entries {
                let entry_path = entry.path();
                if !switchy_fs::unsync::is_dir(&entry_path).await {
                    continue;
                }
                let manifest_path = entry_path.join(manifest_file);
                if !switchy_fs::unsync::exists(&manifest_path).await {
                    continue;
                }
                if let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) {
                    results.push(format!("{base_dir}/{name}"));
                }
            }
        }

        return results;
    }

    // For more complex patterns, use glob matching
    if let Some(matcher) = get_or_compile_glob(pattern) {
        // Walk the directory tree
        let matches = walk_and_match(root, &matcher, manifest_file).await;
        results.extend(matches);
    }

    results
}

/// Walks a directory tree and finds matches for a glob pattern.
async fn walk_and_match(root: &Path, matcher: &GlobMatcher, manifest_file: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut stack = vec![PathBuf::new()]; // Start with empty relative path

    while let Some(rel_path) = stack.pop() {
        let full_path = if rel_path.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(&rel_path)
        };

        // Check if this path matches and has a manifest
        if !rel_path.as_os_str().is_empty() {
            let rel_str = rel_path.to_string_lossy();
            if matcher.is_match(rel_str.as_ref()) {
                let manifest_path = full_path.join(manifest_file);
                if switchy_fs::unsync::exists(&manifest_path).await {
                    results.push(rel_str.into_owned());
                    // Don't descend into matched directories
                    continue;
                }
            }
        }

        // List directory contents
        if let Ok(entries) = switchy_fs::unsync::read_dir_sorted(&full_path).await {
            for entry in entries {
                let entry_path = entry.path();
                if !switchy_fs::unsync::is_dir(&entry_path).await {
                    continue;
                }
                let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                // Skip hidden directories and common non-package directories
                if name.starts_with('.') || name == "node_modules" || name == "target" {
                    continue;
                }
                let child_rel = if rel_path.as_os_str().is_empty() {
                    PathBuf::from(name)
                } else {
                    rel_path.join(name)
                };
                stack.push(child_rel);
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_glob_chars() {
        assert!(contains_glob_chars("packages/*"));
        assert!(contains_glob_chars("packages/**"));
        assert!(contains_glob_chars("pkg_[abc]"));
        assert!(contains_glob_chars("pkg_?"));
        assert!(!contains_glob_chars("packages/foo"));
        assert!(!contains_glob_chars("src/lib"));
    }

    #[test]
    fn test_get_or_compile_glob() {
        let matcher = get_or_compile_glob("packages/*");
        assert!(matcher.is_some());

        let matcher = matcher.unwrap();
        assert!(matcher.is_match("packages/foo"));
        assert!(!matcher.is_match("src/foo"));
    }
}
