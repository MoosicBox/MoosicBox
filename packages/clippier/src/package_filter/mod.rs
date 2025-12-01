//! Generic package filtering based on Cargo.toml properties.
//!
//! This module provides functionality to filter workspace packages based on
//! criteria applied to their Cargo.toml properties.
//!
//! # Examples
//!
//! ## Simple Filtering
//!
//! ```rust,ignore
//! use clippier::package_filter::apply_filters;
//! use std::collections::BTreeMap;
//!
//! // Skip packages where package.publish = false
//! let skip_filters = vec!["package.publish=false".to_string()];
//! let filtered = apply_filters(&packages, &paths, &root, &skip_filters, &[])?;
//! ```
//!
//! ## Complex Expressions with Logical Operators
//!
//! ```rust,ignore
//! // Skip unpublished packages OR examples
//! let skip_filters = vec!["package.publish=false OR package.name$=_example".to_string()];
//!
//! // Include only published moosicbox packages that aren't examples
//! let include_filters = vec!["package.name^=moosicbox_ AND package.publish=true AND NOT package.name$=_example".to_string()];
//!
//! let filtered = apply_filters(&packages, &paths, &root, &skip_filters, &include_filters)?;
//! ```
//!
//! # Filter Syntax
//!
//! Filters use the format: `property[.nested]<operator>value`
//!
//! ## Property Path Resolution
//!
//! Property paths are resolved from the root of Cargo.toml using dot notation.
//! You must specify the full path to any property:
//!
//! * **Package section properties**: Use `package.` prefix
//!   - `package.name`, `package.version`, `package.publish`
//!   - `package.categories`, `package.keywords`, `package.authors`
//!   - `package.metadata.workspaces.independent`
//!   
//! * **Other top-level sections**: Reference directly
//!   - `dependencies.serde.version`
//!   - `features.default`
//!   - `workspace.members`
//!
//! **Examples:**
//! * `package.name^=moosicbox_` - Match packages starting with "moosicbox_"
//! * `dependencies.serde?` - Check if serde is a dependency
//! * `package.metadata.ci.skip=true` - Match custom metadata
//! * `features.default?` - Check if default feature exists
//!
//! ## Logical Operators
//!
//! Combine filter conditions using logical operators:
//!
//! * `AND` - Both conditions must be true
//! * `OR` - At least one condition must be true
//! * `NOT` - Inverts the condition
//! * `( )` - Groups conditions for precedence control
//!
//! **Operator Precedence** (highest to lowest):
//! 1. `NOT`
//! 2. `AND`
//! 3. `OR`
//!
//! **Examples:**
//! * `package.publish=false AND package.version^=0.1` - Both must match
//! * `package.publish=false OR package.name$=_example` - Either must match
//! * `NOT package.publish=false` - Inverts the condition
//! * `(package.publish=false OR package.name$=_test) AND package.version^=0.1` - Grouped with precedence
//! * `package.name^=moosicbox_ AND (package.categories@=audio OR package.categories@=video)` - Complex nesting
//!
//! **Case Insensitive:** Keywords can be `AND`, `and`, `And`, etc.
//!
//! ## Quoted Values
//!
//! Use double quotes for values containing spaces or special characters:
//!
//! * `package.name="my package"` - Value with spaces
//! * `package.description="This AND that"` - Prevents "AND" from being treated as operator
//!
//! **Escape Sequences:**
//! * `\"` - Double quote
//! * `\\` - Backslash
//! * `\n` - Newline
//! * `\t` - Tab
//! * `\r` - Carriage return
//!
//! Example: `package.name="Quote: \"test\""`
//!
//! ## Unicode Support
//!
//! Full Unicode support including multibyte characters, emoji, and RTL text:
//!
//! * Property names with Unicode: `package.名前=test`
//! * Unicode values: `package.name=テスト`
//! * Emoji in values: `package.icon=🎵`
//! * Right-to-left text: `package.اسم=قيمة`
//! * Works with all operators and logical expressions
//!
//! ## Scalar Operators
//!
//! Match against string, boolean, or integer values:
//!
//! * `=` - Exact match: `package.publish=false`
//! * `!=` - Not equal: `package.version!=0.1.0`
//! * `^=` - Starts with: `package.version^=0.1`
//! * `$=` - Ends with: `package.name$=_example`
//! * `*=` - Contains: `package.description*=server`
//! * `~=` - Regex: `package.name~=^moosicbox_.*`
//!
//! ## Array Operators
//!
//! Match against array properties (keywords, categories, authors, etc.):
//!
//! * `@=` - Contains element: `package.categories@=audio`
//! * `@*=` - Contains element with substring: `package.keywords@*=music`
//! * `@^=` - Contains element starting with: `package.keywords@^=moosic`
//! * `@~=` - Contains element matching regex: `package.categories@~=^dev.*`
//! * `@!` - Array is empty: `package.keywords@!`
//! * `@#=` - Length equals: `package.keywords@#=3`
//! * `@#>` - Length greater: `package.keywords@#>2`
//! * `@#<` - Length less: `package.keywords@#<5`
//! * `!@=` - Does NOT contain: `package.categories!@=test`
//!
//! ## Existence Operators
//!
//! Check if properties exist:
//!
//! * `?` - Property exists: `package.readme?`
//! * `!?` - Property does NOT exist: `package.homepage!?`
//!
//! ## Nested Properties
//!
//! Access nested metadata using dot notation:
//!
//! * `package.metadata.workspaces.independent=true`
//! * `package.metadata.ci.skip-tests=true`
//! * `package.metadata.custom.field=value`

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
/// * A simple condition: `"package.publish=false"`
/// * A complex expression: `"(package.publish=false OR package.name$=_example) AND package.categories@=audio"`
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
        let cargo_content = switchy_fs::sync::read_to_string(&cargo_path)
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

    #[test]
    fn test_apply_skip_filter() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Create test package
        let pkg_dir = temp_path.join("test_package");
        switchy_fs::sync::create_dir_all(&pkg_dir).unwrap();
        switchy_fs::sync::write(
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
        let skip_filters = vec!["package.publish=false".to_string()];

        let result =
            apply_filters(&packages, &package_paths, temp_path, &skip_filters, &[]).unwrap();

        assert!(result.is_empty());
    }

    #[test]
    fn test_apply_include_filter() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Create test package
        let pkg_dir = temp_path.join("test_package");
        switchy_fs::sync::create_dir_all(&pkg_dir).unwrap();
        switchy_fs::sync::write(
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
        let include_filters = vec!["package.categories@=audio".to_string()];

        let result =
            apply_filters(&packages, &package_paths, temp_path, &[], &include_filters).unwrap();

        assert_eq!(result, vec!["test_package"]);
    }
}
