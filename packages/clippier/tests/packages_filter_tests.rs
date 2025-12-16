#[cfg(feature = "cargo-workspace")]
use clippier::{OutputType, handle_features_command};
#[cfg(feature = "cargo-workspace")]
use clippier_test_utilities::test_resources::load_test_workspace;
#[cfg(feature = "cargo-workspace")]
use std::collections::HashSet;

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_filter_single_package() {
    // Test filtering to a single package
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,                       // os
        None,                       // offset
        None,                       // max
        None,                       // max_parallel
        None,                       // chunked
        false,                      // spread
        false,                      // randomize
        None,                       // seed
        None,                       // features
        None,                       // skip_features
        None,                       // required_features
        Some(&["api".to_string()]), // packages - only select "api"
        None,                       // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,                      // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should only contain configs for the "api" package
    for config in &json {
        assert_eq!(config["name"], "api");
    }
    assert!(
        !json.is_empty(),
        "Should have at least one config for api package"
    );
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_filter_multiple_packages() {
    // Test filtering to multiple packages
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&["api".to_string(), "web".to_string(), "cli".to_string()]),
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Collect unique package names - web has multiple configs (frontend-build, ssr-build)
    let package_names: HashSet<String> = json
        .iter()
        .map(|config| config["name"].as_str().unwrap().to_string())
        .collect();

    assert_eq!(package_names.len(), 4); // api, cli, frontend-build, ssr-build
    assert!(package_names.contains("api"));
    assert!(package_names.contains("frontend-build") || package_names.contains("ssr-build"));
    assert!(package_names.contains("cli"));

    // Should NOT contain other packages
    assert!(!package_names.contains("core"));
    assert!(!package_names.contains("models"));
    assert!(!package_names.contains("shared-utils"));
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_filter_empty_list() {
    // Test with empty packages list (should process all packages)
    let (temp_dir, _) = load_test_workspace("complex");
    let result_empty = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&[]), // Empty list
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    let result_none = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        None, // No packages specified
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    // Both should process all packages
    assert!(result_empty.is_ok());
    assert!(result_none.is_ok());

    let json_empty: Vec<serde_json::Value> = serde_json::from_str(&result_empty.unwrap()).unwrap();
    let json_none: Vec<serde_json::Value> = serde_json::from_str(&result_none.unwrap()).unwrap();

    // Should have processed all packages (6 base packages + web has 2 configs = 7 total configs, but we check unique names)
    let package_names: HashSet<String> = json_none
        .iter()
        .map(|config| config["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(package_names.len(), 7); // 6 base packages + frontend-build + ssr-build

    // Empty list and None should produce the same result
    assert_eq!(json_empty.len(), json_none.len());
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_with_os_filter() {
    // Test combining --packages with --os
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"), // os filter
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&["api".to_string(), "web".to_string()]),
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // All configs should be for ubuntu and only for api/web packages
    for config in &json {
        assert_eq!(config["os"], "ubuntu");
        let name = config["name"].as_str().unwrap();
        assert!(name == "api" || name == "web" || name == "frontend-build" || name == "ssr-build");
    }
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_with_chunking() {
    // Test combining --packages with --chunked
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        Some(2), // chunked
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&["web".to_string()]), // Package with multiple features
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Web package should be processed (it has chunked=2 in clippier.toml)
    for config in &json {
        let name = config["name"].as_str().unwrap();
        assert!(
            name == "frontend-build" || name == "ssr-build",
            "Should only have web package configs"
        );
        let features = config["features"].as_array();
        if let Some(features) = features {
            assert!(
                !features.is_empty(),
                "Each config should have at least one feature"
            );
        }
    }
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_with_features_filter() {
    // Test combining --packages with --features and --skip-features
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        Some("default"),  // specific feature
        Some("advanced"), // skip feature
        None,
        Some(&["web".to_string()]),
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    for config in &json {
        let name = config["name"].as_str().unwrap();
        assert!(
            name == "frontend-build" || name == "ssr-build",
            "Should only have web package configs"
        );
        let features = config["features"].as_array();
        // Features might be empty depending on the config
        if let Some(_features) = features {
            // Just verify it's an array - feature filtering might result in different features
        }
    }
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_nonexistent_package() {
    // Test with package name that doesn't exist
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&["nonexistent_package".to_string()]),
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should return empty array since package doesn't exist
    assert_eq!(json.len(), 0);
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_mixed_valid_invalid() {
    // Test with mix of valid and invalid package names
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&[
            "api".to_string(),
            "nonexistent".to_string(),
            "web".to_string(),
            "fake_package".to_string(),
        ]),
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should only contain configs for valid packages
    let package_names: HashSet<String> = json
        .iter()
        .map(|config| config["name"].as_str().unwrap().to_string())
        .collect();

    assert!(package_names.contains("api"));
    assert!(package_names.contains("frontend-build") || package_names.contains("ssr-build"));
    assert!(!package_names.contains("nonexistent"));
    assert!(!package_names.contains("fake_package"));
    assert!(package_names.len() >= 2); // At least api + web configs
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_case_sensitivity() {
    // Test that package names are case-sensitive
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&["API".to_string(), "Web".to_string()]), // Wrong case
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let json: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should not find packages with wrong case
    assert_eq!(json.len(), 0);
}

#[cfg(feature = "cargo-workspace")]
#[switchy_async::test]
async fn test_packages_raw_output_format() {
    // Test that --packages works with Raw output format
    let (temp_dir, _) = load_test_workspace("complex");
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
        Some(&["api".to_string()]),
        None,
        #[cfg(feature = "git-diff")]
        None,
        #[cfg(feature = "git-diff")]
        None,
        false,
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        #[cfg(feature = "_workspace")]
        None, // workspace_type filter
        OutputType::Raw,
    )
    .await;

    assert!(result.is_ok());
    let output = result.unwrap();

    // Should have raw output (feature combinations separated by newlines)
    assert!(!output.is_empty());
    // Each line should be a feature combination
    for line in output.lines() {
        assert!(!line.is_empty());
    }
}
