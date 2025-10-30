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

mod expression_parser;
mod matcher;
mod parser;
pub mod tokenizer;
mod types;

pub use expression_parser::parse_expression;
pub use matcher::{evaluate_expression, matches};
pub use parser::parse_filter;
pub use tokenizer::tokenize;
pub use types::{FilterError, FilterExpression, FilterOperator, PackageFilter, Token};

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
/// * `skip_filters` - Filter expressions that cause packages to be excluded (OR logic between filters)
/// * `include_filters` - Filter expressions that must match for inclusion (AND logic between filters)
///
/// Each filter can be:
/// * A simple condition: `"publish=false"`
/// * A complex expression: `"(publish=false OR name$=_example) AND categories@=audio"`
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

        // Check skip filters - if ANY expression matches, skip package (OR logic)
        let should_skip = skip_filters.iter().any(|filter_expr| {
            expression_parser::parse_expression(filter_expr)
                .and_then(|expr| matcher::evaluate_expression(&expr, &cargo_toml))
                .unwrap_or(false)
        });

        if should_skip {
            continue;
        }

        // Check include filters - ALL expressions must match (AND logic)
        let should_include = if include_filters.is_empty() {
            true
        } else {
            include_filters.iter().all(|filter_expr| {
                expression_parser::parse_expression(filter_expr)
                    .and_then(|expr| matcher::evaluate_expression(&expr, &cargo_toml))
                    .unwrap_or(false)
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
