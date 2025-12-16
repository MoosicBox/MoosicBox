//! pnpm pnpm-lock.yaml parser.

use super::{NodeLockEntry, NodeLockfile};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Parses a pnpm pnpm-lock.yaml file.
///
/// # Errors
///
/// Returns an error if the YAML is invalid or doesn't match expected structure.
pub fn parse_pnpm_lockfile(content: &str) -> Result<NodeLockfile, BoxError> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(content)?;

    let mut entries = Vec::new();

    // pnpm lockfile has "packages" section with keys like "/@scope/name@version" or "/name@version"
    if let Some(packages) = yaml.get("packages").and_then(serde_yaml::Value::as_mapping) {
        for (key, info) in packages {
            if let Some(key_str) = key.as_str()
                && let Some((name, version)) = parse_pnpm_package_key(key_str)
            {
                let dependencies = extract_pnpm_dependencies(info);

                entries.push(NodeLockEntry {
                    name,
                    version,
                    dependencies,
                });
            }
        }
    }

    // Also check "snapshots" section (pnpm v9+)
    if let Some(snapshots) = yaml
        .get("snapshots")
        .and_then(serde_yaml::Value::as_mapping)
    {
        for (key, info) in snapshots {
            if let Some(key_str) = key.as_str()
                && let Some((name, version)) = parse_pnpm_package_key(key_str)
                // Check if we already have this package
                && !entries.iter().any(|e| e.name == name && e.version == version)
            {
                let dependencies = extract_pnpm_dependencies(info);

                entries.push(NodeLockEntry {
                    name,
                    version,
                    dependencies,
                });
            }
        }
    }

    Ok(NodeLockfile { entries })
}

/// Parses a pnpm package key to extract name and version.
///
/// Formats:
/// - `/name@version`
/// - `/@scope/name@version`
/// - `/name@version(peer1@version)(peer2@version)` (with peer deps)
fn parse_pnpm_package_key(key: &str) -> Option<(String, String)> {
    let key = key.strip_prefix('/')?;

    // Remove peer dependency suffix if present: name@version(peer@ver) -> name@version
    let key = key.find('(').map_or(key, |paren_pos| &key[..paren_pos]);

    // Handle scoped packages: @scope/name@version
    if key.starts_with('@') {
        // Find the slash after scope
        let slash_pos = key.find('/')?;
        // Find the @ after the package name
        let at_pos = key[slash_pos + 1..].find('@')? + slash_pos + 1;

        let name = &key[..at_pos];
        let version = &key[at_pos + 1..];

        Some((name.to_string(), version.to_string()))
    } else {
        // Non-scoped: name@version
        let at_pos = key.find('@')?;
        let name = &key[..at_pos];
        let version = &key[at_pos + 1..];

        Some((name.to_string(), version.to_string()))
    }
}

/// Extracts dependencies from a pnpm package entry.
fn extract_pnpm_dependencies(info: &serde_yaml::Value) -> Vec<String> {
    let mut deps = Vec::new();

    for section in ["dependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(section_deps) = info.get(section).and_then(serde_yaml::Value::as_mapping) {
            for (name, _) in section_deps {
                if let Some(name_str) = name.as_str() {
                    deps.push(name_str.to_string());
                }
            }
        }
    }

    deps
}

/// Parses pnpm lockfile diff to extract changed package names.
#[must_use]
pub fn parse_pnpm_lock_changes(changes: &[(char, String)]) -> Vec<String> {
    let mut changed_packages = std::collections::BTreeSet::new();
    let mut in_packages_section = false;
    let mut current_package: Option<String> = None;
    let mut has_version_change = false;

    for (op, line) in changes {
        let line_trimmed = line.trim();

        // Detect packages section
        if line_trimmed == "packages:" || line_trimmed == "snapshots:" {
            in_packages_section = true;
            continue;
        }

        // Detect end of packages section (new top-level key)
        if in_packages_section
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && line_trimmed.ends_with(':')
        {
            in_packages_section = false;
            continue;
        }

        if !in_packages_section {
            continue;
        }

        // Detect package entry (line starting with / or ' /)
        // Format: "  /lodash@4.17.21:" or "  /@scope/name@version:"
        let stripped = line_trimmed.trim_start_matches(['\'', '"'].as_ref());
        if stripped.starts_with('/') && stripped.ends_with(':') {
            let key = stripped
                .trim_end_matches(':')
                .trim_end_matches(['\'', '"'].as_ref());
            if let Some((name, _)) = parse_pnpm_package_key(key) {
                // Save previous package if it had changes
                if let Some(prev_name) = &current_package
                    && (has_version_change || *op == '+')
                {
                    changed_packages.insert(prev_name.clone());
                }

                current_package = Some(name);
                has_version_change = *op == '+' || *op == '-';
            }
        } else if line_trimmed.starts_with("version:") && (*op == '+' || *op == '-') {
            has_version_change = true;
        }
    }

    // Handle last package
    if let Some(name) = current_package
        && has_version_change
    {
        changed_packages.insert(name);
    }

    changed_packages.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pnpm_package_key_simple() {
        let result = parse_pnpm_package_key("/lodash@4.17.21");
        assert_eq!(result, Some(("lodash".to_string(), "4.17.21".to_string())));
    }

    #[test]
    fn test_parse_pnpm_package_key_scoped() {
        let result = parse_pnpm_package_key("/@babel/core@7.23.0");
        assert_eq!(
            result,
            Some(("@babel/core".to_string(), "7.23.0".to_string()))
        );
    }

    #[test]
    fn test_parse_pnpm_package_key_with_peers() {
        let result = parse_pnpm_package_key("/react-dom@18.2.0(react@18.2.0)");
        assert_eq!(
            result,
            Some(("react-dom".to_string(), "18.2.0".to_string()))
        );
    }

    #[test]
    fn test_parse_pnpm_lockfile() {
        let content = r"
lockfileVersion: '9.0'

packages:
  /lodash@4.17.21:
    resolution: {integrity: sha512-abc}
    dev: true

  /@babel/core@7.23.0:
    resolution: {integrity: sha512-def}
    dependencies:
      '@babel/helper-compilation-targets': 7.22.15
";

        let lockfile = parse_pnpm_lockfile(content).unwrap();

        assert_eq!(lockfile.entries.len(), 2);

        let lodash = lockfile.find_by_name("lodash").unwrap();
        assert_eq!(lodash.version, "4.17.21");

        let babel = lockfile.find_by_name("@babel/core").unwrap();
        assert_eq!(babel.version, "7.23.0");
        assert!(
            babel
                .dependencies
                .contains(&"@babel/helper-compilation-targets".to_string())
        );
    }

    #[test]
    fn test_parse_pnpm_lock_changes() {
        let changes = vec![
            (' ', "packages:".to_string()),
            (' ', "  /lodash@4.17.20:".to_string()),
            ('-', "    version: 4.17.20".to_string()),
            ('+', "  /lodash@4.17.21:".to_string()),
            ('+', "    version: 4.17.21".to_string()),
        ];

        let result = parse_pnpm_lock_changes(&changes);
        assert!(result.contains(&"lodash".to_string()));
    }
}
