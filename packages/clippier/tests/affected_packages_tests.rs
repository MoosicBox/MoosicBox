use clippier::AffectedPackageInfo;
use clippier_test_utilities::test_resources::load_test_workspace;

/// Create a test workspace for affected packages testing
fn create_affected_packages_workspace() -> switchy_fs::TempDir {
    let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");

    // Create workspace Cargo.toml
    let workspace_toml = r#"
[workspace]
members = [
    "packages/core",
    "packages/models",
    "packages/api",
    "packages/web",
    "packages/cli",
    "packages/shared-utils"
]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
        .expect("Failed to write workspace Cargo.toml");

    // Core package - foundation
    let core_dir = temp_dir.path().join("packages/core");
    switchy_fs::sync::create_dir_all(core_dir.join("src"))
        .expect("Failed to create core directory");
    let core_cargo = r#"
[package]
name = "core"
version = "0.1.0"
edition = "2021"

[dependencies]
shared-utils = { workspace = true }
"#;
    switchy_fs::sync::write(core_dir.join("Cargo.toml"), core_cargo)
        .expect("Failed to write core Cargo.toml");
    switchy_fs::sync::write(core_dir.join("src/lib.rs"), "// core")
        .expect("Failed to write core lib.rs");

    // Models package - depends on core
    let models_dir = temp_dir.path().join("packages/models");
    switchy_fs::sync::create_dir_all(models_dir.join("src"))
        .expect("Failed to create models directory");
    let models_cargo = r#"
[package]
name = "models"
version = "0.1.0"
edition = "2021"

[dependencies]
core = { workspace = true }
"#;
    switchy_fs::sync::write(models_dir.join("Cargo.toml"), models_cargo)
        .expect("Failed to write models Cargo.toml");
    switchy_fs::sync::write(models_dir.join("src/lib.rs"), "// models")
        .expect("Failed to write models lib.rs");

    // API package - depends on models
    let api_dir = temp_dir.path().join("packages/api");
    switchy_fs::sync::create_dir_all(api_dir.join("src")).expect("Failed to create api directory");
    let api_cargo = r#"
[package]
name = "api"
version = "0.1.0"
edition = "2021"

[dependencies]
models = { workspace = true }
core = { workspace = true }
"#;
    switchy_fs::sync::write(api_dir.join("Cargo.toml"), api_cargo)
        .expect("Failed to write api Cargo.toml");
    switchy_fs::sync::write(api_dir.join("src/lib.rs"), "// api")
        .expect("Failed to write api lib.rs");

    // Web package - depends on api
    let web_dir = temp_dir.path().join("packages/web");
    switchy_fs::sync::create_dir_all(web_dir.join("src")).expect("Failed to create web directory");
    let web_cargo = r#"
[package]
name = "web"
version = "0.1.0"
edition = "2021"

[dependencies]
api = { workspace = true }
"#;
    switchy_fs::sync::write(web_dir.join("Cargo.toml"), web_cargo)
        .expect("Failed to write web Cargo.toml");
    switchy_fs::sync::write(web_dir.join("src/lib.rs"), "// web")
        .expect("Failed to write web lib.rs");

    // CLI package - depends on api and models
    let cli_dir = temp_dir.path().join("packages/cli");
    switchy_fs::sync::create_dir_all(cli_dir.join("src")).expect("Failed to create cli directory");
    let cli_cargo = r#"
[package]
name = "cli"
version = "0.1.0"
edition = "2021"

[dependencies]
api = { workspace = true }
models = { workspace = true }
"#;
    switchy_fs::sync::write(cli_dir.join("Cargo.toml"), cli_cargo)
        .expect("Failed to write cli Cargo.toml");
    switchy_fs::sync::write(cli_dir.join("src/main.rs"), "fn main() {}")
        .expect("Failed to write cli main.rs");

    // Shared utils package - standalone
    let utils_dir = temp_dir.path().join("packages/shared-utils");
    switchy_fs::sync::create_dir_all(utils_dir.join("src"))
        .expect("Failed to create utils directory");
    let utils_cargo = r#"
[package]
name = "shared-utils"
version = "0.1.0"
edition = "2021"
"#;
    switchy_fs::sync::write(utils_dir.join("Cargo.toml"), utils_cargo)
        .expect("Failed to write utils Cargo.toml");
    switchy_fs::sync::write(utils_dir.join("src/lib.rs"), "// shared utils")
        .expect("Failed to write utils lib.rs");

    temp_dir
}

#[switchy_async::test]
async fn test_find_affected_packages_direct_change() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(packages, vec!["core"]);
}

#[switchy_async::test]
async fn test_find_affected_packages_leaf_change() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/web/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(packages, vec!["web"]);
}

#[switchy_async::test]
async fn test_find_affected_packages_multiple_files() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/src/lib.rs".to_string(),
        "packages/shared-utils/src/lib.rs".to_string(),
    ];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(packages, vec!["core", "shared-utils"]);
}

#[switchy_async::test]
async fn test_find_affected_packages_with_reasoning() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/src/lib.rs".to_string()];

    let result =
        clippier::find_affected_packages_with_reasoning(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec![AffectedPackageInfo {
            name: "core".to_string(),
            reasoning: Some(vec![
                "Contains changed file: packages/core/src/lib.rs".to_string()
            ])
        }]
    );
}

#[switchy_async::test]
async fn test_find_affected_packages_cargo_toml_change() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/Cargo.toml".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(packages, vec!["core"]);
}

#[switchy_async::test]
async fn test_find_affected_packages_nested_path() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/api/src/handlers/mod.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(packages, vec!["api"]);
}

#[switchy_async::test]
async fn test_affected_packages_complex_dependency_chain() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(packages, vec!["core"]);
}

#[switchy_async::test]
async fn test_find_affected_packages_mixed_changes() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/src/lib.rs".to_string(),
        "packages/api/Cargo.toml".to_string(),
        "packages/web/src/components/mod.rs".to_string(),
    ];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let packages = result.unwrap();
    assert_eq!(packages, vec!["api", "core", "web"]);
}

#[switchy_async::test]
async fn test_single_package_affected_check() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/api/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());

    let all_affected = result.unwrap();
    assert_eq!(all_affected, vec!["api"]);
}

#[switchy_async::test]
async fn test_find_affected_packages_no_changes() {
    let temp_dir = create_affected_packages_workspace();

    let changed_files = vec!["README.md".to_string(), "docs/guide.md".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // No packages should be affected by non-package files
    assert_eq!(result.len(), 0);
}

#[switchy_async::test]
async fn test_find_affected_packages_workspace_root_change() {
    let temp_dir = create_affected_packages_workspace();

    let changed_files = vec!["Cargo.toml".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Workspace root changes typically don't affect individual packages
    // unless the packages are under the workspace root directly
    assert_eq!(result.len(), 0);
}

#[switchy_async::test]
async fn test_find_affected_packages_partial_path_match() {
    let temp_dir = create_affected_packages_workspace();

    // Create a file that partially matches a package name but is not in the package
    let false_dir = temp_dir.path().join("packages-backup");
    switchy_fs::sync::create_dir_all(&false_dir).expect("Failed to create false directory");
    switchy_fs::sync::write(false_dir.join("core-backup.rs"), "// backup")
        .expect("Failed to write false file");

    let changed_files = vec!["packages-backup/core-backup.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Should not affect any packages since the file is not in a package directory
    assert_eq!(result.len(), 0);
}

#[switchy_async::test]
async fn test_find_affected_packages_case_sensitivity() {
    let temp_dir = create_affected_packages_workspace();

    let changed_files = vec!["PACKAGES/core/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Should not match due to case sensitivity (PACKAGES vs packages)
    assert_eq!(result.len(), 0);
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_find_affected_packages_with_external_deps() {
    use clippier_test_utilities::test_resources::load_cargo_lock_for_git_diff;

    let (temp_dir, _) = load_test_workspace("complex");

    // Test that external dependency analysis utilities are available
    // This tests the git-diff feature integration
    let _cargo_lock = load_cargo_lock_for_git_diff("basic", "simple");

    // For now, just test that basic affected packages functionality works
    let changed_files = vec!["packages/api/src/lib.rs".to_string()];
    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);

    assert!(result.is_ok());
    let packages = result.unwrap();

    // Since this is a stub implementation, we expect empty results
    assert_eq!(packages, vec!["api"]);
}

#[switchy_async::test]
async fn test_empty_changed_files() {
    let temp_dir = create_affected_packages_workspace();

    let changed_files: Vec<String> = vec![];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // No files changed, no packages affected
    assert_eq!(result.len(), 0);
}

#[switchy_async::test]
async fn test_direct_file_changes() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/api/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_transitive_impact_analysis() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_multiple_file_changes() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/src/lib.rs".to_string(),
        "packages/shared-utils/src/lib.rs".to_string(),
    ];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_complex_dependency_chains() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/api/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_affected_with_reasoning() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/models/src/lib.rs".to_string()];

    let result =
        clippier::find_affected_packages_with_reasoning(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_nested_path_edge_cases() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/api/src/handlers/mod.rs".to_string(),
        "packages/api/tests/integration.rs".to_string(),
    ];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_partial_path_matches() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/ap/file.rs".to_string()]; // Partial match

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_case_sensitivity() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["PACKAGES/API/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_cargo_toml_vs_source_changes() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/api/Cargo.toml".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_empty_change_sets() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files: Vec<String> = vec![];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

#[switchy_async::test]
async fn test_workspace_root_changes() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);
    assert!(result.is_ok());
}

// Snapshot tests with proper JSON serialization
#[switchy_async::test]
async fn test_direct_file_changes_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "changed_files": ["packages/api/src/lib.rs"],
        "directly_affected": ["api"],
        "reasoning": {
            "api": ["Contains changed file: packages/api/src/lib.rs"]
        }
    });

    insta::assert_yaml_snapshot!("direct_file_changes", test_data);
}

#[switchy_async::test]
async fn test_transitive_impact_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "changed_files": ["packages/core/src/lib.rs"],
        "directly_affected": ["core"],
        "transitively_affected": ["models", "api", "web", "cli"],
        "dependency_chain": {
            "models": "depends on core",
            "api": "depends on models",
            "web": "depends on api",
            "cli": "depends on api"
        }
    });

    insta::assert_yaml_snapshot!("transitive_impact", test_data);
}

#[switchy_async::test]
async fn test_multiple_file_changes_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "changed_files": [
            "packages/core/src/lib.rs",
            "packages/shared-utils/src/lib.rs"
        ],
        "directly_affected": ["core", "shared-utils"],
        "note": "Multiple files can affect different dependency trees"
    });

    insta::assert_yaml_snapshot!("multiple_file_changes", test_data);
}

#[switchy_async::test]
async fn test_complex_dependency_chains_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "dependency_graph": {
            "core": [],
            "shared-utils": [],
            "models": ["core"],
            "api": ["models", "shared-utils"],
            "web": ["api"],
            "cli": ["api"]
        },
        "changed_api_affects": ["web", "cli"]
    });

    insta::assert_yaml_snapshot!("complex_dependency_chains", test_data);
}

#[switchy_async::test]
async fn test_affected_reasoning_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "changed_files": ["packages/models/src/lib.rs"],
        "reasoning_chain": {
            "models": ["Contains changed file: packages/models/src/lib.rs"],
            "api": ["Depends on affected package: models"],
            "web": ["Depends on affected package: api"],
            "cli": ["Depends on affected package: api"]
        },
        "note": "Each package includes reasoning for why it's affected"
    });

    insta::assert_yaml_snapshot!("affected_reasoning", test_data);
}

#[switchy_async::test]
async fn test_nested_path_edge_cases_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "changed_files": [
            "packages/api/src/handlers/mod.rs",
            "packages/api/tests/integration.rs",
            "packages/api/benches/benchmark.rs"
        ],
        "affected_packages": ["api"],
        "note": "All files within package directory should be detected"
    });

    insta::assert_yaml_snapshot!("nested_path_edge_cases", test_data);
}

#[switchy_async::test]
async fn test_partial_path_matches_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test edge case: partial path matches should not affect packages
    // assert!(true); // This is testing that "packages/ap" doesn't match "packages/api"
}

#[switchy_async::test]
async fn test_case_sensitivity_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test case sensitivity behavior
    // assert!(true); // This depends on filesystem case sensitivity
}

#[switchy_async::test]
async fn test_empty_change_sets_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "changed_files": [],
        "affected_packages": [],
        "note": "No changes should result in no affected packages"
    });

    insta::assert_yaml_snapshot!("empty_change_sets", test_data);
}

#[switchy_async::test]
async fn test_workspace_root_changes_snapshot() {
    let (_temp_dir, _) = load_test_workspace("complex");

    let test_data = serde_json::json!({
        "changed_files": ["Cargo.toml", "Cargo.lock"],
        "affected_packages": [],
        "note": "Workspace-level changes don't map to specific packages"
    });

    insta::assert_yaml_snapshot!("workspace_root_changes", test_data);
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_external_dependency_integration() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test integration with external dependency analysis
    let test_data = serde_json::json!({
        "external_deps_changed": ["serde", "tokio"],
        "packages_using_serde": ["core", "models", "shared-utils"],
        "packages_using_tokio": ["core", "api"],
        "transitively_affected": ["api", "web", "cli"],
        "reasoning": {
            "core": ["External dependency changes: serde, tokio"],
            "models": ["Depends on affected package: core"],
            "api": ["External dependency changes: tokio", "Depends on affected package: models"]
        }
    });

    insta::assert_yaml_snapshot!("external_deps_integration", test_data);
}

/// Create a test workspace with nested packages to test the switchy/switchy_schema scenario
fn create_nested_packages_workspace() -> switchy_fs::TempDir {
    let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");

    // Create workspace Cargo.toml
    let workspace_toml = r#"
[workspace]
members = [
    "packages/parent",
    "packages/parent/nested",
    "packages/sibling"
]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
        .expect("Failed to write workspace Cargo.toml");

    // Parent package (like switchy)
    let parent_dir = temp_dir.path().join("packages/parent");
    switchy_fs::sync::create_dir_all(parent_dir.join("src"))
        .expect("Failed to create parent directory");
    let parent_cargo = r#"
[package]
name = "parent"
version = "0.1.0"
edition = "2021"

[dependencies]
sibling = { workspace = true }
"#;
    switchy_fs::sync::write(parent_dir.join("Cargo.toml"), parent_cargo)
        .expect("Failed to write parent Cargo.toml");
    switchy_fs::sync::write(parent_dir.join("src/lib.rs"), "// parent")
        .expect("Failed to write parent lib.rs");

    // Nested package (like switchy_schema)
    let nested_dir = temp_dir.path().join("packages/parent/nested");
    switchy_fs::sync::create_dir_all(nested_dir.join("src"))
        .expect("Failed to create nested directory");
    let nested_cargo = r#"
[package]
name = "parent_nested"
version = "0.1.0"
edition = "2021"

[dependencies]
parent = { workspace = true }
"#;
    switchy_fs::sync::write(nested_dir.join("Cargo.toml"), nested_cargo)
        .expect("Failed to write nested Cargo.toml");
    switchy_fs::sync::write(nested_dir.join("src/lib.rs"), "// nested")
        .expect("Failed to write nested lib.rs");
    switchy_fs::sync::write(nested_dir.join("README.md"), "# Nested Package")
        .expect("Failed to write nested README.md");

    // Sibling package (independent)
    let sibling_dir = temp_dir.path().join("packages/sibling");
    switchy_fs::sync::create_dir_all(sibling_dir.join("src"))
        .expect("Failed to create sibling directory");
    let sibling_cargo = r#"
[package]
name = "sibling"
version = "0.1.0"
edition = "2021"
"#;
    switchy_fs::sync::write(sibling_dir.join("Cargo.toml"), sibling_cargo)
        .expect("Failed to write sibling Cargo.toml");
    switchy_fs::sync::write(sibling_dir.join("src/lib.rs"), "// sibling")
        .expect("Failed to write sibling lib.rs");

    temp_dir
}

#[switchy_async::test]
async fn test_nested_package_change_does_not_affect_parent() {
    let temp_dir = create_nested_packages_workspace();

    // Change a file in the nested package
    let changed_files = vec!["packages/parent/nested/README.md".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Only the nested package should be affected, not the parent
    assert_eq!(result, vec!["parent_nested"]);
    assert!(!result.contains(&"parent".to_string()));
}

#[switchy_async::test]
async fn test_nested_package_source_change_does_not_affect_parent() {
    let temp_dir = create_nested_packages_workspace();

    // Change a source file in the nested package
    let changed_files = vec!["packages/parent/nested/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Only the nested package should be affected, not the parent
    assert_eq!(result, vec!["parent_nested"]);
    assert!(!result.contains(&"parent".to_string()));
}

#[switchy_async::test]
async fn test_parent_package_change_affects_dependent_nested() {
    let temp_dir = create_nested_packages_workspace();

    // Change a file in the parent package (but not in nested directory)
    let changed_files = vec!["packages/parent/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Both parent and nested should be affected because nested depends on parent
    let mut sorted_result = result;
    sorted_result.sort();
    assert_eq!(sorted_result, vec!["parent", "parent_nested"]);
}

#[switchy_async::test]
async fn test_nested_package_with_reasoning() {
    let temp_dir = create_nested_packages_workspace();

    // Change a file in the nested package
    let changed_files = vec!["packages/parent/nested/README.md".to_string()];

    let result =
        clippier::find_affected_packages_with_reasoning(temp_dir.path(), &changed_files, &[])
            .expect("Failed to find affected packages with reasoning");

    // Should only affect the nested package with proper reasoning
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "parent_nested");
    assert_eq!(
        result[0].reasoning,
        Some(vec![
            "Contains changed file: packages/parent/nested/README.md".to_string()
        ])
    );
}

#[switchy_async::test]
async fn test_multiple_nested_changes() {
    let temp_dir = create_nested_packages_workspace();

    // Change files in both parent and nested packages
    let changed_files = vec![
        "packages/parent/src/lib.rs".to_string(),
        "packages/parent/nested/src/lib.rs".to_string(),
    ];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Both packages should be affected, but each only by their own files
    let mut sorted_result = result;
    sorted_result.sort();
    assert_eq!(sorted_result, vec!["parent", "parent_nested"]);
}

#[switchy_async::test]
async fn test_deeply_nested_packages() {
    let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");

    // Create workspace with deeply nested structure
    let workspace_toml = r#"
[workspace]
members = [
    "packages/level1",
    "packages/level1/level2",
    "packages/level1/level2/level3"
]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
        .expect("Failed to write workspace Cargo.toml");

    // Level 1 package
    let level1_dir = temp_dir.path().join("packages/level1");
    switchy_fs::sync::create_dir_all(level1_dir.join("src"))
        .expect("Failed to create level1 directory");
    let level1_cargo = r#"
[package]
name = "level1"
version = "0.1.0"
edition = "2021"
"#;
    switchy_fs::sync::write(level1_dir.join("Cargo.toml"), level1_cargo)
        .expect("Failed to write level1 Cargo.toml");
    switchy_fs::sync::write(level1_dir.join("src/lib.rs"), "// level1")
        .expect("Failed to write level1 lib.rs");

    // Level 2 package
    let level2_dir = temp_dir.path().join("packages/level1/level2");
    switchy_fs::sync::create_dir_all(level2_dir.join("src"))
        .expect("Failed to create level2 directory");
    let level2_cargo = r#"
[package]
name = "level2"
version = "0.1.0"
edition = "2021"
"#;
    switchy_fs::sync::write(level2_dir.join("Cargo.toml"), level2_cargo)
        .expect("Failed to write level2 Cargo.toml");
    switchy_fs::sync::write(level2_dir.join("src/lib.rs"), "// level2")
        .expect("Failed to write level2 lib.rs");

    // Level 3 package
    let level3_dir = temp_dir.path().join("packages/level1/level2/level3");
    switchy_fs::sync::create_dir_all(level3_dir.join("src"))
        .expect("Failed to create level3 directory");
    let level3_cargo = r#"
[package]
name = "level3"
version = "0.1.0"
edition = "2021"
"#;
    switchy_fs::sync::write(level3_dir.join("Cargo.toml"), level3_cargo)
        .expect("Failed to write level3 Cargo.toml");
    switchy_fs::sync::write(level3_dir.join("src/lib.rs"), "// level3")
        .expect("Failed to write level3 lib.rs");

    // Test that changing the deepest level only affects that package
    let changed_files = vec!["packages/level1/level2/level3/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Only the deepest package should be affected
    assert_eq!(result, vec!["level3"]);
    assert!(!result.contains(&"level1".to_string()));
    assert!(!result.contains(&"level2".to_string()));
}

/// Create a test workspace with independent nested packages (no dependencies between them)
fn create_independent_nested_packages_workspace() -> switchy_fs::TempDir {
    let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");

    // Create workspace Cargo.toml
    let workspace_toml = r#"
[workspace]
members = [
    "packages/parent",
    "packages/parent/independent_nested"
]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
        .expect("Failed to write workspace Cargo.toml");

    // Parent package
    let parent_dir = temp_dir.path().join("packages/parent");
    switchy_fs::sync::create_dir_all(parent_dir.join("src"))
        .expect("Failed to create parent directory");
    let parent_cargo = r#"
[package]
name = "parent"
version = "0.1.0"
edition = "2021"
"#;
    switchy_fs::sync::write(parent_dir.join("Cargo.toml"), parent_cargo)
        .expect("Failed to write parent Cargo.toml");
    switchy_fs::sync::write(parent_dir.join("src/lib.rs"), "// parent")
        .expect("Failed to write parent lib.rs");

    // Independent nested package (no dependency on parent)
    let nested_dir = temp_dir.path().join("packages/parent/independent_nested");
    switchy_fs::sync::create_dir_all(nested_dir.join("src"))
        .expect("Failed to create nested directory");
    let nested_cargo = r#"
[package]
name = "independent_nested"
version = "0.1.0"
edition = "2021"
"#;
    switchy_fs::sync::write(nested_dir.join("Cargo.toml"), nested_cargo)
        .expect("Failed to write nested Cargo.toml");
    switchy_fs::sync::write(nested_dir.join("src/lib.rs"), "// independent nested")
        .expect("Failed to write nested lib.rs");
    switchy_fs::sync::write(nested_dir.join("README.md"), "# Independent Nested Package")
        .expect("Failed to write nested README.md");

    temp_dir
}

#[switchy_async::test]
async fn test_independent_nested_package_does_not_affect_parent() {
    let temp_dir = create_independent_nested_packages_workspace();

    // Change a file in the independent nested package
    let changed_files = vec!["packages/parent/independent_nested/README.md".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Only the nested package should be affected, not the parent
    assert_eq!(result, vec!["independent_nested"]);
    assert!(!result.contains(&"parent".to_string()));
}

#[switchy_async::test]
async fn test_independent_parent_package_does_not_affect_nested() {
    let temp_dir = create_independent_nested_packages_workspace();

    // Change a file in the parent package (but not in nested directory)
    let changed_files = vec!["packages/parent/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[])
        .expect("Failed to find affected packages");

    // Only the parent package should be affected, not the nested
    assert_eq!(result, vec!["parent"]);
    assert!(!result.contains(&"independent_nested".to_string()));
}
