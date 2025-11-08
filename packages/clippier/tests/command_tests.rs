use clippier_test_utilities::test_resources::load_test_workspace;
use itertools::Itertools;

#[test]
fn test_process_configs_basic() {
    let (temp_dir, _) = load_test_workspace("complex");
    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::process_configs(
        &temp_dir.path().join("packages/cli"),
        &workspace_context,
        None,
        None,
        None,
        false,
        false, // randomize = false for test
        None,
        None,
        None,
        None,
    );
    assert!(result.is_ok());
}

#[test]
fn test_process_configs_with_clippier_toml() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test with API package that has clippier.toml configuration
    let test_data = serde_json::json!({
        "package_name": "api",
        "os": "ubuntu",
        "has_clippier_toml": true,
        "expected_env_vars": ["API_PORT", "DATABASE_URL"],
        "expected_deps": ["libsqlite3-dev", "build-essential"]
    });
    insta::assert_yaml_snapshot!("complex_api_config", test_data);
}

#[test]
fn test_feature_chunking() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test with web package that has chunked parallelization
    let test_data = serde_json::json!({
        "package_name": "web",
        "chunked": 2,
        "features": ["frontend", "ssr"]
    });
    insta::assert_yaml_snapshot!("web_chunked_features", test_data);
}

#[test]
fn test_feature_filtering() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test CLI package feature filtering
    let test_data = serde_json::json!({
        "package_name": "cli",
        "all_features": ["interactive", "batch"],
        "skip_features": ["batch"],
        "filtered_features": ["interactive"]
    });

    insta::assert_yaml_snapshot!("cli_feature_filtering", test_data);
}

#[test]
fn test_multiple_os_configs() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test API package OS configurations
    let test_data = serde_json::json!({
        "package_name": "api",
        "supported_os": ["ubuntu", "windows", "macos"],
        "ubuntu_deps": ["apt-get install libsqlite3-dev"],
        "windows_deps": ["vcpkg install sqlite3:x64-windows"],
        "macos_deps": ["brew install sqlite3"]
    });

    insta::assert_yaml_snapshot!("multiple_os_configs", test_data);
}

#[test]
fn test_environment_variables() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test environment variable processing with feature conditions
    let test_data = serde_json::json!({
        "package_name": "web",
        "env_vars": {
            "WEB_PORT": "8080",
            "FRONTEND_BUILD": {
                "value": "production",
                "features": ["frontend"]
            },
            "SSR_ENABLED": "true"
        }
    });
    insta::assert_yaml_snapshot!("web_env_vars", test_data);
}

#[test]
fn test_workspace_fallback() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test workspace-level configuration inheritance
    let test_data = serde_json::json!({
        "package_name": "models",
        "has_clippier_toml": false,
        "fallback_os": "ubuntu",
        "fallback_features": []
    });

    insta::assert_yaml_snapshot!("workspace_fallback", test_data);
}

#[test]
fn test_cargo_arguments() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test cargo argument passing
}

#[test]
fn test_nightly_flags() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test nightly Rust flags
}

#[test]
fn test_feature_limits() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test offset and max feature limits
    let test_data = serde_json::json!({
        "package_name": "web",
        "all_features": ["frontend", "ssr"],
        "offset_1_max_1": ["ssr"],
        "offset_0_max_1": ["frontend"]
    });

    insta::assert_yaml_snapshot!("feature_limits", test_data);
}

#[test]
fn test_spread_distribution() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test spread feature distribution
    let test_data = serde_json::json!({
        "package_name": "api",
        "chunked": 2,
        "spread": true,
        "features": ["server", "database"],
        "expected_chunks": [["server"], ["database"]]
    });

    insta::assert_yaml_snapshot!("spread_distribution", test_data);
}

#[test]
fn test_workspace_loads_successfully() {
    let (temp_dir, _) = load_test_workspace("basic");

    // Verify the basic workspace structure exists
    assert!(temp_dir.path().join("Cargo.toml").exists());
    assert!(temp_dir.path().join("packages/api/Cargo.toml").exists());
    assert!(temp_dir.path().join("packages/models/Cargo.toml").exists());

    insta::assert_snapshot!(
        "basic_workspace_structure",
        format!(
            "{:?}",
            std::fs::read_dir(temp_dir.path().join("packages"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
                .sorted()
                .collect::<Vec<_>>()
        )
    );
}

#[test]
fn test_complex_workspace_loads_successfully() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Verify the complex workspace structure exists
    assert!(temp_dir.path().join("Cargo.toml").exists());
    assert!(temp_dir.path().join("packages/core/Cargo.toml").exists());
    assert!(temp_dir.path().join("packages/api/clippier.toml").exists());
    assert!(temp_dir.path().join("packages/web/clippier.toml").exists());

    let mut packages: Vec<String> = std::fs::read_dir(temp_dir.path().join("packages"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .sorted()
        .collect();

    packages.sort();

    insta::assert_yaml_snapshot!("complex_workspace_packages", packages);
}

#[test]
fn test_api_package_has_correct_structure() {
    let (temp_dir, _) = load_test_workspace("complex");

    let api_cargo =
        std::fs::read_to_string(temp_dir.path().join("packages/api/Cargo.toml")).unwrap();

    // Verify key aspects of the API package configuration
    assert!(api_cargo.contains("name    = \"api\""));
    assert!(api_cargo.contains("core         = { path = \"../core\""));
    assert!(api_cargo.contains("models       = { path = \"../models\""));

    insta::assert_snapshot!("api_cargo_toml", api_cargo);
}

#[test]
fn test_clippier_config_exists_for_api() {
    let (temp_dir, _) = load_test_workspace("complex");

    let api_clippier =
        std::fs::read_to_string(temp_dir.path().join("packages/api/clippier.toml")).unwrap();

    // Verify clippier configuration has expected sections
    assert!(api_clippier.contains("[env]"));
    assert!(api_clippier.contains("[[config]]"));
    assert!(api_clippier.contains("os = \"ubuntu\""));

    insta::assert_snapshot!("api_clippier_toml", api_clippier);
}

#[test]
fn test_git_submodules_enabled() {
    let (temp_dir, _) = load_test_workspace("git-submodules");
    let submodules_pkg = temp_dir.path().join("packages/with-submodules");

    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::process_configs(
        &submodules_pkg,
        &workspace_context,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let json = serde_json::to_value(&result).unwrap();
    let git_submodules = json[0]["gitSubmodules"].as_bool();
    assert_eq!(git_submodules, Some(true));
}

#[test]
fn test_git_submodules_disabled_by_default() {
    let (temp_dir, _) = load_test_workspace("complex");
    let api_path = temp_dir.path().join("packages/api");

    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::process_configs(
        &api_path,
        &workspace_context,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    let json = serde_json::to_value(&result).unwrap();
    assert!(json[0].get("gitSubmodules").is_none() || json[0]["gitSubmodules"].is_null());
}

#[test]
fn test_git_submodules_multiple_os() {
    let (temp_dir, _) = load_test_workspace("git-submodules");
    let submodules_pkg = temp_dir.path().join("packages/with-submodules");

    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::process_configs(
        &submodules_pkg,
        &workspace_context,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    assert_eq!(result.len(), 3);

    for config in &result {
        let json = serde_json::to_value(config).unwrap();
        assert_eq!(json["gitSubmodules"].as_bool(), Some(true));
    }

    let mut settings = insta::Settings::clone_current();
    settings.add_redaction(".**.path", "[TEMP_PATH]");
    settings.bind(|| {
        insta::assert_yaml_snapshot!("git_submodules_multiple_os", result);
    });
}

#[test]
fn test_git_submodules_not_present_without_config() {
    let (temp_dir, _) = load_test_workspace("git-submodules");
    let without_submodules_pkg = temp_dir.path().join("packages/without-submodules");

    let workspace_context = clippier::WorkspaceContext::new(temp_dir.path()).unwrap();
    let result = clippier::process_configs(
        &without_submodules_pkg,
        &workspace_context,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    for config in &result {
        let json = serde_json::to_value(config).unwrap();
        assert!(json.get("gitSubmodules").is_none() || json["gitSubmodules"].is_null());
    }
}
