//! npm package-lock.json parser.

use super::{NodeLockEntry, NodeLockfile};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Parses an npm package-lock.json file.
///
/// Supports both lockfileVersion 2/3 (packages section) and v1 (dependencies section).
///
/// # Errors
///
/// Returns an error if the JSON is invalid or doesn't match expected structure.
pub fn parse_npm_lockfile(content: &str) -> Result<NodeLockfile, BoxError> {
    let json: serde_json::Value = serde_json::from_str(content)?;

    let version = json
        .get("lockfileVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);

    let entries = if version >= 2 {
        parse_packages_section(&json)?
    } else {
        parse_dependencies_section(&json)?
    };

    Ok(NodeLockfile { entries })
}

/// Parses the "packages" section (lockfileVersion >= 2).
fn parse_packages_section(json: &serde_json::Value) -> Result<Vec<NodeLockEntry>, BoxError> {
    let packages = json
        .get("packages")
        .and_then(|p| p.as_object())
        .ok_or("Missing packages section")?;

    let mut entries = Vec::new();

    for (path, info) in packages {
        // Skip the root package (empty path)
        if path.is_empty() {
            continue;
        }

        // Extract package name from path: "node_modules/@scope/name" -> "@scope/name"
        let name = path.strip_prefix("node_modules/").map_or_else(
            || path.as_str(),
            |stripped| {
                // Handle nested node_modules (dependency of dependency)
                // e.g., "node_modules/foo/node_modules/bar" -> "bar"
                stripped
                    .rfind("node_modules/")
                    .map_or(stripped, |last_nm| &stripped[last_nm + 13..])
            },
        );

        if let Some(version) = info.get("version").and_then(serde_json::Value::as_str) {
            let dependencies = extract_dependencies(info);

            entries.push(NodeLockEntry {
                name: name.to_string(),
                version: version.to_string(),
                dependencies,
            });
        }
    }

    Ok(entries)
}

/// Parses the "dependencies" section (lockfileVersion 1).
#[allow(clippy::unnecessary_wraps)]
fn parse_dependencies_section(json: &serde_json::Value) -> Result<Vec<NodeLockEntry>, BoxError> {
    let Some(dependencies) = json
        .get("dependencies")
        .and_then(serde_json::Value::as_object)
    else {
        return Ok(Vec::new());
    };

    let mut entries = Vec::new();
    parse_deps_recursive(dependencies, &mut entries);

    Ok(entries)
}

/// Recursively parses nested dependencies.
fn parse_deps_recursive(
    deps: &serde_json::Map<String, serde_json::Value>,
    entries: &mut Vec<NodeLockEntry>,
) {
    for (name, info) in deps {
        if let Some(version) = info.get("version").and_then(serde_json::Value::as_str) {
            let dependencies = extract_dependencies(info);

            entries.push(NodeLockEntry {
                name: name.clone(),
                version: version.to_string(),
                dependencies,
            });

            // Recurse into nested dependencies
            if let Some(nested) = info
                .get("dependencies")
                .and_then(serde_json::Value::as_object)
            {
                parse_deps_recursive(nested, entries);
            }
        }
    }
}

/// Extracts dependency names from a package entry.
fn extract_dependencies(info: &serde_json::Value) -> Vec<String> {
    let mut deps = Vec::new();

    for section in ["dependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(section_deps) = info.get(section).and_then(serde_json::Value::as_object) {
            deps.extend(section_deps.keys().cloned());
        }
    }

    // Also check "requires" field (used in older lockfile formats)
    if let Some(requires) = info.get("requires").and_then(serde_json::Value::as_object) {
        deps.extend(requires.keys().cloned());
    }

    deps
}

/// Parses npm lockfile diff to extract changed package names.
#[must_use]
pub fn parse_npm_lock_changes(changes: &[(char, String)]) -> Vec<String> {
    let mut changed_packages = std::collections::BTreeSet::new();
    let mut current_path: Option<String> = None;
    let mut has_version_change = false;
    let mut is_new_entry = false;
    let mut in_packages_section = false;

    for (op, line) in changes {
        let line = line.trim();

        // Detect packages section
        if line.contains("\"packages\"") {
            in_packages_section = true;
            continue;
        }

        if !in_packages_section {
            continue;
        }

        // Detect package path (key in packages object)
        // Pattern: "node_modules/package-name": {
        if line.contains("\"node_modules/")
            && line.contains("\": {")
            && let Some(start) = line.find("\"node_modules/")
            && let Some(end) = line[start + 14..].find('"')
        {
            let path = &line[start + 14..start + 14 + end];
            current_path = Some(extract_package_name_from_path(path));
            has_version_change = false;
            is_new_entry = *op == '+';
        } else if line.starts_with("\"version\"") && (*op == '-' || *op == '+') {
            has_version_change = true;
        } else if line == "}," || line == "}" {
            // End of entry
            if let Some(name) = &current_path
                && (has_version_change || is_new_entry)
            {
                changed_packages.insert(name.clone());
            }
            current_path = None;
            has_version_change = false;
            is_new_entry = false;
        }
    }

    changed_packages.into_iter().collect()
}

/// Extracts package name from a `node_modules` path.
fn extract_package_name_from_path(path: &str) -> String {
    // Handle nested node_modules
    path.rfind("node_modules/").map_or_else(
        || path.to_string(),
        |last_nm| path[last_nm + 13..].to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_npm_lockfile_v3() {
        let content = r#"
        {
            "name": "my-project",
            "version": "1.0.0",
            "lockfileVersion": 3,
            "packages": {
                "": {
                    "name": "my-project",
                    "version": "1.0.0"
                },
                "node_modules/lodash": {
                    "version": "4.17.21",
                    "resolved": "https://registry.npmjs.org/lodash/-/lodash-4.17.21.tgz"
                },
                "node_modules/express": {
                    "version": "4.18.2",
                    "dependencies": {
                        "accepts": "~1.3.8"
                    }
                }
            }
        }
        "#;

        let lockfile = parse_npm_lockfile(content).unwrap();

        assert_eq!(lockfile.entries.len(), 2);

        let lodash = lockfile.find_by_name("lodash").unwrap();
        assert_eq!(lodash.version, "4.17.21");

        let express = lockfile.find_by_name("express").unwrap();
        assert_eq!(express.version, "4.18.2");
        assert!(express.dependencies.contains(&"accepts".to_string()));
    }

    #[test]
    fn test_parse_npm_lock_changes() {
        let changes = vec![
            (' ', "  \"packages\": {".to_string()),
            (' ', "    \"node_modules/lodash\": {".to_string()),
            ('-', "      \"version\": \"4.17.20\"".to_string()),
            ('+', "      \"version\": \"4.17.21\"".to_string()),
            (' ', "    },".to_string()),
        ];

        let result = parse_npm_lock_changes(&changes);
        assert_eq!(result, vec!["lodash"]);
    }

    #[test]
    fn test_extract_package_name_scoped() {
        assert_eq!(extract_package_name_from_path("@babel/core"), "@babel/core");
        assert_eq!(
            extract_package_name_from_path("node_modules/@types/node"),
            "@types/node"
        );
    }
}
