use std::fs;

use clippier::{
    OutputType, handle_ci_steps_command, handle_dependencies_command, handle_environment_command,
    handle_features_command, handle_workspace_deps_command, process_workspace_configs,
};
use clippier_test_utilities::test_resources::{create_simple_workspace, load_test_workspace};
use tempfile::TempDir;

/// Test the handle_dependencies_command function with various scenarios
#[test]
fn test_handle_dependencies_command_basic() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let dependencies = result.unwrap();

    insta::assert_snapshot!("dependencies_command_complex_ubuntu", dependencies);
}

#[test]
fn test_handle_dependencies_command_json_output() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Json,
    );

    assert!(result.is_ok());
    let dependencies_json = result.unwrap();

    // Parse JSON to ensure it's valid
    let parsed: serde_json::Value = serde_json::from_str(&dependencies_json).unwrap();
    insta::assert_yaml_snapshot!("dependencies_command_json_output", parsed);
}

#[test]
fn test_handle_dependencies_command_with_features() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        Some("frontend,ssr"),
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let dependencies = result.unwrap();

    insta::assert_snapshot!("dependencies_command_with_features", dependencies);
}

#[test]
fn test_handle_dependencies_command_no_os_filter() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let dependencies = result.unwrap();

    insta::assert_snapshot!("dependencies_command_all_os", dependencies);
}

#[test]
fn test_handle_dependencies_command_single_package() {
    let (temp_dir, _) = load_test_workspace("complex");
    let api_path = temp_dir.path().join("packages/api");

    let result = handle_dependencies_command(
        api_path.to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let dependencies = result.unwrap();

    insta::assert_snapshot!("dependencies_command_single_package", dependencies);
}

#[test]
fn test_handle_dependencies_command_empty_result() {
    let (temp_dir, _) =
        create_simple_workspace(&["package1"], &["serde"], &[("package1", &["serde"])]);

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let dependencies = result.unwrap();

    // Should be empty since no clippier.toml files exist
    assert!(dependencies.is_empty());
}

/// Test the handle_environment_command function
#[test]
fn test_handle_environment_command_basic() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_environment_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let env_vars = result.unwrap();

    insta::assert_snapshot!("environment_command_complex_ubuntu", env_vars);
}

#[test]
fn test_handle_environment_command_json_output() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_environment_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Json,
    );

    assert!(result.is_ok());
    let env_vars_json = result.unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&env_vars_json).unwrap();
    insta::assert_yaml_snapshot!("environment_command_json_output", parsed);
}

#[test]
fn test_handle_environment_command_with_features() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_environment_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        Some("frontend"),
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let env_vars = result.unwrap();

    insta::assert_snapshot!("environment_command_with_features", env_vars);
}

/// Test the handle_ci_steps_command function
#[test]
fn test_handle_ci_steps_command_basic() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_ci_steps_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let ci_steps = result.unwrap();

    insta::assert_snapshot!("ci_steps_command_complex_ubuntu", ci_steps);
}

#[test]
fn test_handle_ci_steps_command_json_output() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_ci_steps_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Json,
    );

    assert!(result.is_ok());
    let ci_steps_json = result.unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&ci_steps_json).unwrap();
    insta::assert_yaml_snapshot!("ci_steps_command_json_output", parsed);
}

/// Test the handle_features_command function
#[test]
fn test_handle_features_command_basic() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,  // offset
        None,  // max
        None,  // max_parallel
        None,  // chunked
        false, // spread
        false, // randomize
        None,  // seed
        None,  // features
        None,  // skip_features
        None,  // required_features
        None,  // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false, // include_reasoning
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let features = result.unwrap();

    insta::assert_snapshot!("features_command_complex_ubuntu", features);
}

/// Test the handle_workspace_deps_command function
#[test]
fn test_handle_workspace_deps_command_basic() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_workspace_deps_command(temp_dir.path(), "api", None, "text", false);

    assert!(result.is_ok());
    let deps = result.unwrap();

    insta::assert_snapshot!("workspace_deps_command_api", deps);
}

/// Test the process_workspace_configs function
#[test]
fn test_process_workspace_configs_workspace_root() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = process_workspace_configs(
        temp_dir.path(),
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        None,
        None,
    );

    assert!(result.is_ok());
    let configs = result.unwrap();

    // Just verify basic structure rather than using snapshots due to changing paths
    assert!(!configs.is_empty());

    // Verify that we get configs for multiple OS types
    let ubuntu_configs: Vec<_> = configs
        .iter()
        .filter(|c| c.get("os").and_then(|v| v.as_str()) == Some("ubuntu"))
        .collect();
    assert!(!ubuntu_configs.is_empty());

    let windows_configs: Vec<_> = configs
        .iter()
        .filter(|c| c.get("os").and_then(|v| v.as_str()) == Some("windows"))
        .collect();
    assert!(!windows_configs.is_empty());

    // Verify that we get configs for multiple packages
    let api_configs: Vec<_> = configs
        .iter()
        .filter(|c| c.get("name").and_then(|v| v.as_str()) == Some("api"))
        .collect();
    assert!(!api_configs.is_empty());

    let web_configs: Vec<_> = configs
        .iter()
        .filter(|c| {
            c.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("build")
        })
        .collect();
    assert!(!web_configs.is_empty());
}

/// Test deduplication behavior specifically
#[test]
fn test_dependencies_deduplication() {
    // Create a workspace with duplicate dependencies across packages
    let temp_dir = create_test_workspace_with_duplicate_deps();

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let dependencies = result.unwrap();

    insta::assert_snapshot!("dependencies_deduplication", dependencies);
}

/// Helper function to create a test workspace with duplicate dependencies
fn create_test_workspace_with_duplicate_deps() -> TempDir {
    use std::fs;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create workspace Cargo.toml
    let workspace_toml = r#"
[workspace]
members = ["packages/pkg1", "packages/pkg2", "packages/pkg3"]

[workspace.dependencies]
serde = "1.0"
"#;
    fs::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages with clippier.toml files containing duplicate dependencies
    for (i, pkg) in ["pkg1", "pkg2", "pkg3"].iter().enumerate() {
        let pkg_dir = temp_dir.path().join("packages").join(pkg);
        fs::create_dir_all(pkg_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = {{ workspace = true }}
"#
        );
        fs::write(pkg_dir.join("Cargo.toml"), cargo_toml).unwrap();
        fs::write(pkg_dir.join("src/lib.rs"), "// test lib").unwrap();

        // Create clippier.toml with same dependencies (should be deduplicated)
        let clippier_toml = if i == 0 {
            r#"
[[config]]
os = "ubuntu"
dependencies = [
    { command = "sudo apt-get update && sudo apt-get install build-essential" },
    { command = "sudo apt-get update && sudo apt-get install libssl-dev" }
]
"#
        } else {
            r#"
[[config]]
os = "ubuntu"
dependencies = [
    { command = "sudo apt-get update && sudo apt-get install build-essential" },
    { command = "sudo apt-get update && sudo apt-get install curl" }
]
"#
        };
        fs::write(pkg_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    temp_dir
}

/// Test comprehensive scenario with multiple features and chunking
#[test]
fn test_handle_features_command_comprehensive() {
    let (temp_dir, _) = load_test_workspace("complex");

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        Some(0),              // offset
        Some(3),              // max
        None,                 // max_parallel
        Some(2),              // chunked
        true,                 // spread
        false,                // randomize
        None,                 // seed
        Some("frontend,api"), // features
        Some("deprecated"),   // skip_features
        Some("core"),         // required_features
        None,                 // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,                // include_reasoning
        OutputType::Json,
    );

    assert!(result.is_ok());
    let features_json = result.unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&features_json).unwrap();

    // Instead of snapshots with changing paths, verify structure
    assert!(parsed.is_array(), "Result should be an array");
    let configs = parsed.as_array().unwrap();
    assert!(!configs.is_empty(), "Should have configurations");

    // Verify specific features filtering worked
    let has_frontend = configs.iter().any(|config| {
        config
            .get("features")
            .and_then(|f| f.as_array())
            .unwrap_or(&vec![])
            .iter()
            .any(|feature| feature.as_str() == Some("frontend"))
    });
    assert!(has_frontend, "Should have configs with frontend feature");

    let has_api = configs.iter().any(|config| {
        config
            .get("features")
            .and_then(|f| f.as_array())
            .unwrap_or(&vec![])
            .iter()
            .any(|feature| feature.as_str() == Some("api"))
    });
    assert!(has_api, "Should have configs with api feature");

    // Verify required features are present
    let has_required_core = configs.iter().all(|config| {
        config
            .get("requiredFeatures")
            .and_then(|f| f.as_array())
            .unwrap_or(&vec![])
            .iter()
            .any(|feature| feature.as_str() == Some("core"))
    });
    assert!(
        has_required_core,
        "All configs should have core as required feature"
    );
}

#[test]
fn test_handle_features_command_max_parallel_limits_results() {
    let temp_dir = tempfile::tempdir().unwrap();

    // Create workspace with multiple packages to generate many results
    for i in 1..=20 {
        let pkg = format!("package{i}");
        let pkg_dir = temp_dir.path().join("packages").join(&pkg);
        fs::create_dir_all(pkg_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg}"
version = "0.1.0"

[features]
default = []
feature1 = []
feature2 = []
feature3 = []
feature4 = []
feature5 = []

[dependencies]
serde = {{ workspace = true }}
"#
        );
        fs::write(pkg_dir.join("Cargo.toml"), cargo_toml).unwrap();
        fs::write(pkg_dir.join("src/lib.rs"), "// test lib").unwrap();

        // Create clippier.toml for each package
        let clippier_toml = r#"
[[config]]
os = "ubuntu"
dependencies = [
    { command = "apt-get install -y build-essential" }
]
"#;
        fs::write(pkg_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    // Create workspace Cargo.toml
    let members: Vec<String> = (1..=20).map(|i| format!("packages/package{i}")).collect();
    let workspace_toml = format!(
        r#"
[workspace]
members = [{}]

[workspace.dependencies]
serde = "1.0"
"#,
        members
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    fs::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Test with both chunked and max_parallel
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,     // offset
        None,     // max
        Some(10), // max_parallel - should limit to 10 results
        Some(3),  // chunked - should create more than 10 results without limit
        false,    // spread
        false,    // randomize
        None,     // seed
        None,     // features
        None,     // skip_features
        None,     // required_features
        None,     // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,    // include_reasoning
        OutputType::Json,
    );

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should be limited to exactly 10 results by max_parallel
    assert_eq!(configs.len(), 10, "max_parallel should limit results to 10");

    // Verify that features are preserved (not truncated) - count total features
    let total_features: usize = configs
        .iter()
        .filter_map(|config| config.get("features"))
        .filter_map(|features| features.as_array())
        .map(|arr| arr.len())
        .sum();

    // There should be a significant number of features preserved (each package has 6 features: default + 5 named)
    assert!(
        total_features > 0,
        "Features should be preserved during re-chunking"
    );

    // Test with only max_parallel (backward compatibility)
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        Some(5), // max_parallel - should limit to 5 results and also serve as chunked
        None,    // chunked - not provided
        false,   // spread
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        OutputType::Json,
    );

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should be limited to exactly 5 results by max_parallel
    assert_eq!(
        configs.len(),
        5,
        "max_parallel without chunked should limit results to 5"
    );

    // Verify that features are preserved in this case too
    let total_features: usize = configs
        .iter()
        .filter_map(|config| config.get("features"))
        .filter_map(|features| features.as_array())
        .map(|arr| arr.len())
        .sum();

    assert!(
        total_features > 0,
        "Features should be preserved during re-chunking"
    );
}

/// Test error handling and edge cases
#[test]
fn test_handle_commands_error_handling() {
    // Test with completely invalid path
    let result = handle_dependencies_command(
        "/this/path/definitely/does/not/exist",
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );
    assert!(result.is_err());

    // Test with empty workspace
    let (temp_dir, _) = create_simple_workspace(&[], &[], &[]);

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

/// Test output format consistency across different commands
#[test]
fn test_output_format_consistency() {
    let (temp_dir, _) = load_test_workspace("complex");
    let path = temp_dir.path().to_str().unwrap();

    // Test dependencies command
    let deps_raw = handle_dependencies_command(path, Some("ubuntu"), None, OutputType::Raw);
    assert!(deps_raw.is_ok(), "Dependencies raw output failed");

    let deps_json = handle_dependencies_command(path, Some("ubuntu"), None, OutputType::Json);
    assert!(deps_json.is_ok(), "Dependencies JSON output failed");

    let deps_json_str = deps_json.unwrap();
    if !deps_json_str.is_empty() {
        let _parsed: serde_json::Value =
            serde_json::from_str(&deps_json_str).expect("Invalid JSON from dependencies command");
    }

    // Test environment command
    let env_raw = handle_environment_command(path, Some("ubuntu"), None, OutputType::Raw);
    assert!(env_raw.is_ok(), "Environment raw output failed");

    let env_json = handle_environment_command(path, Some("ubuntu"), None, OutputType::Json);
    assert!(env_json.is_ok(), "Environment JSON output failed");

    let env_json_str = env_json.unwrap();
    if !env_json_str.is_empty() {
        let _parsed: serde_json::Value =
            serde_json::from_str(&env_json_str).expect("Invalid JSON from environment command");
    }

    // Test CI steps command
    let ci_raw = handle_ci_steps_command(path, Some("ubuntu"), None, OutputType::Raw);
    assert!(ci_raw.is_ok(), "CI steps raw output failed");

    let ci_json = handle_ci_steps_command(path, Some("ubuntu"), None, OutputType::Json);
    assert!(ci_json.is_ok(), "CI steps JSON output failed");

    let ci_json_str = ci_json.unwrap();
    if !ci_json_str.is_empty() {
        let _parsed: serde_json::Value =
            serde_json::from_str(&ci_json_str).expect("Invalid JSON from CI steps command");
    }
}

/// Test that deduplication works correctly with identical multiline blocks
#[test]
fn test_exact_deduplication_behavior() {
    let temp_dir = create_test_workspace_with_exact_duplicates();

    let result = handle_dependencies_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        OutputType::Raw,
    );

    assert!(result.is_ok());
    let dependencies = result.unwrap();

    // Split by lines and count occurrences
    let lines: Vec<&str> = dependencies.lines().collect();

    // Should only have 2 unique multiline blocks, not 4 (due to deduplication)
    // The exact same multiline dependency block should appear only once
    assert_eq!(
        lines.len(),
        2,
        "Expected exactly 2 unique dependency lines after deduplication"
    );

    insta::assert_snapshot!("exact_deduplication_result", dependencies);
}

/// Helper function to create exact duplicates for testing deduplication
fn create_test_workspace_with_exact_duplicates() -> TempDir {
    use std::fs;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create workspace Cargo.toml
    let workspace_toml = r#"
[workspace]
members = ["packages/pkg1", "packages/pkg2"]

[workspace.dependencies]
serde = "1.0"
"#;
    fs::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages with EXACTLY the same dependencies (should be deduplicated)
    for pkg in ["pkg1", "pkg2"] {
        let pkg_dir = temp_dir.path().join("packages").join(pkg);
        fs::create_dir_all(pkg_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = {{ workspace = true }}
"#
        );
        fs::write(pkg_dir.join("Cargo.toml"), cargo_toml).unwrap();
        fs::write(pkg_dir.join("src/lib.rs"), "// test lib").unwrap();

        // Create clippier.toml with EXACTLY the same dependencies
        let clippier_toml = r#"
[[config]]
os = "ubuntu"
dependencies = [
    { command = "apt-get update && apt-get install -y build-essential" },
    { command = "apt-get update && apt-get install -y libssl-dev" }
]
"#;
        fs::write(pkg_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    temp_dir
}
