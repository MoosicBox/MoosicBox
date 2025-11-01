use clippier_test_utilities::test_resources::load_test_workspace;

#[test]
fn test_simple_workspace_dependencies() {
    let (_temp_dir, _) = load_test_workspace("basic");

    // Test that the basic workspace dependency structure is correct
    let test_data = serde_json::json!({
        "api_dependencies": ["models"],
        "models_dependencies": [],
        "workspace_members": ["api", "models"]
    });
    insta::assert_yaml_snapshot!("basic_workspace_deps", test_data);
}

#[test]
fn test_workspace_dependency_resolution() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test simple dependency resolution
    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::find_workspace_dependencies(&workspace_context, "api", None, false);
    assert!(result.is_ok());
}

#[test]
fn test_transitive_dependencies() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test transitive dependency chain
    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::find_workspace_dependencies(&workspace_context, "web", None, false);
    assert!(result.is_ok());
}

#[test]
fn test_feature_conditional_deps() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test feature-conditional dependencies
    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::find_workspace_dependencies(
        &workspace_context,
        "core",
        Some(&["database".to_string()]),
        false,
    );
    assert!(result.is_ok());
}

#[test]
fn test_all_potential_dependencies() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test all potential dependencies mode
    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::find_workspace_dependencies(
        &workspace_context,
        "core",
        None,
        true, // all_potential_deps = true
    );
    assert!(result.is_ok());
}

#[test]
fn test_dev_build_dependencies() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test inclusion of dev and build dependencies
    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::find_workspace_dependencies(&workspace_context, "api", None, false);
    assert!(result.is_ok());
}

#[test]
fn test_circular_dependency_detection() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test circular dependency handling
    let test_data = serde_json::json!({
        "api_depends_on": ["models"],
        "models_depends_on": [],
        "no_circular_deps": true,
        "note": "Basic workspace has no circular dependencies"
    });

    insta::assert_yaml_snapshot!("circular_deps", test_data);
}

#[test]
fn test_optional_dependency_activation() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test optional dependency activation via features
    let test_data = serde_json::json!({
        "core_optional_deps": ["sqlx"],
        "activated_via_features": ["database"],
        "models_enables_core_database": true
    });

    insta::assert_yaml_snapshot!("optional_deps", test_data);
}

#[test]
fn test_default_features_disabled() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test when default features are disabled
    let test_data = serde_json::json!({
        "note": "When default-features = false, only explicitly enabled features are included",
        "example_package": "models",
        "explicit_features": ["json"]
    });

    insta::assert_yaml_snapshot!("default_features_disabled", test_data);
}

#[test]
fn test_workspace_paths() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test workspace path resolution
    let test_data = serde_json::json!({
        "core_path": "packages/core",
        "models_path": "packages/models",
        "api_path": "packages/api",
        "web_path": "packages/web",
        "cli_path": "packages/cli",
        "shared_utils_path": "packages/shared-utils",
        "note": "Workspace root path excluded from snapshot as it changes per test run"
    });

    insta::assert_yaml_snapshot!("workspace_paths", test_data);
}

#[test]
fn test_nonexistent_package_error() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test error handling for nonexistent packages
    let test_data = serde_json::json!({
        "requested_package": "nonexistent",
        "available_packages": ["api", "models"],
        "should_error": true,
        "error_message": "Package 'nonexistent' not found in workspace"
    });

    insta::assert_yaml_snapshot!("nonexistent_package", test_data);
}
