//! Generic package filtering based on Cargo.toml properties.
//!
//! This module provides functionality to filter workspace packages based on
//! criteria applied to their Cargo.toml properties.
//!
//! # Examples
//!
//! ```rust,ignore
//! use clippier::package_filter::apply_filters;
//! use std::collections::BTreeMap;
//!
//! // Skip packages where publish = false
//! let skip_filters = vec!["publish=false".to_string()];
//! let filtered = apply_filters(&packages, &paths, &root, &skip_filters, &[])?;
//! ```
//!
//! # Filter Syntax
//!
//! Filters use the format: `property[.nested]<operator>value`
//!
//! ## Scalar Operators
//!
//! * `=` - Exact match: `publish=false`
//! * `!=` - Not equal: `version!=0.1.0`
//! * `^=` - Starts with: `version^=0.1`
//! * `$=` - Ends with: `name$=_example`
//! * `*=` - Contains: `description*=server`
//! * `~=` - Regex: `name~=^moosicbox_.*`
//!
//! ## Array Operators
//!
//! * `@=` - Contains element: `categories@=audio`
//! * `@*=` - Contains element with substring: `keywords@*=music`
//! * `@^=` - Contains element starting with: `keywords@^=moosic`
//! * `@~=` - Contains element matching regex: `categories@~=^dev.*`
//! * `@!` - Array is empty: `keywords@!`
//! * `@#=` - Length equals: `keywords@#=3`
//! * `@#>` - Length greater: `keywords@#>2`
//! * `@#<` - Length less: `keywords@#<5`
//! * `!@=` - Does NOT contain: `categories!@=test`
//!
//! ## Existence Operators
//!
//! * `?` - Property exists: `readme?`
//! * `!?` - Property does NOT exist: `!homepage?`

mod matcher;
mod parser;
mod types;

pub use matcher::matches;
pub use parser::parse_filter;
pub use types::{FilterError, FilterOperator, PackageFilter};

use std::collections::BTreeMap;
use std::path::Path;
use toml::Value;

/// Apply skip and include filters to a list of packages.
///
/// # Arguments
///
/// * `packages` - List of package names to filter
/// * `package_paths` - Map of package names to their workspace paths
/// * `workspace_root` - Root directory of the workspace
/// * `skip_filters` - Filters that cause packages to be excluded (OR logic)
/// * `include_filters` - Filters that must match for inclusion (AND logic between properties, OR within)
///
/// # Returns
///
/// Filtered list of package names that pass all criteria
///
/// # Errors
///
/// Returns error if:
/// * Package path not found
/// * Cargo.toml cannot be read or parsed
/// * Filter syntax is invalid
pub fn apply_filters(
    packages: &[String],
    package_paths: &BTreeMap<String, String>,
    workspace_root: &Path,
    skip_filters: &[String],
    include_filters: &[String],
) -> Result<Vec<String>, FilterError> {
    let mut result = Vec::new();

    for package_name in packages {
        let package_path = package_paths.get(package_name).ok_or_else(|| {
            FilterError::PropertyNotFound(format!("Package path not found: {package_name}"))
        })?;

        let cargo_path = workspace_root.join(package_path).join("Cargo.toml");
        let cargo_content = std::fs::read_to_string(&cargo_path)
            .map_err(|e| FilterError::IoError(e.to_string()))?;
        let cargo_toml: Value =
            toml::from_str(&cargo_content).map_err(|e| FilterError::TomlError(e.to_string()))?;

        // Check skip filters - if ANY match, skip package
        let should_skip = skip_filters.iter().any(|filter| {
            parser::parse_filter(filter)
                .and_then(|parsed| matcher::matches(&parsed, &cargo_toml))
                .unwrap_or(false)
        });

        if should_skip {
            continue;
        }

        // Check include filters - group by property path root
        let should_include = if include_filters.is_empty() {
            true
        } else {
            // Parse all include filters
            let parsed_filters: Vec<_> = include_filters
                .iter()
                .filter_map(|f| parser::parse_filter(f).ok())
                .collect();

            // Group by root property (first segment of path)
            let mut filters_by_root: BTreeMap<String, Vec<&PackageFilter>> = BTreeMap::new();
            for filter in &parsed_filters {
                let root = filter
                    .property_path
                    .first()
                    .map_or("package", String::as_str);
                filters_by_root
                    .entry(root.to_string())
                    .or_default()
                    .push(filter);
            }

            // For each root property, at least ONE filter must match (OR within root)
            // ALL root properties must have a match (AND between roots)
            filters_by_root.values().all(|root_filters| {
                root_filters
                    .iter()
                    .any(|filter| matcher::matches(filter, &cargo_toml).unwrap_or(false))
            })
        };

        if should_include {
            result.push(package_name.clone());
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    #[test]
    fn test_apply_skip_filter() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create test package
        let pkg_dir = temp_path.join("test_package");
        std::fs::create_dir(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("Cargo.toml"),
            r#"
            [package]
            name = "test_package"
            version = "0.1.0"
            publish = false
        "#,
        )
        .unwrap();

        let mut package_paths = BTreeMap::new();
        package_paths.insert("test_package".to_string(), "test_package".to_string());

        let packages = vec!["test_package".to_string()];
        let skip_filters = vec!["publish=false".to_string()];

        let result =
            apply_filters(&packages, &package_paths, temp_path, &skip_filters, &[]).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_include_filter() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create test package
        let pkg_dir = temp_path.join("test_package");
        std::fs::create_dir(&pkg_dir).unwrap();
        std::fs::write(
            pkg_dir.join("Cargo.toml"),
            r#"
            [package]
            name = "test_package"
            version = "0.1.0"
            categories = ["audio"]
        "#,
        )
        .unwrap();

        let mut package_paths = BTreeMap::new();
        package_paths.insert("test_package".to_string(), "test_package".to_string());

        let packages = vec!["test_package".to_string()];
        let include_filters = vec!["categories@=audio".to_string()];

        let result =
            apply_filters(&packages, &package_paths, temp_path, &[], &include_filters).unwrap();

        assert_eq!(result, vec!["test_package"]);
    }
}
