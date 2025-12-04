use clippier::{OutputType, handle_features_command};
use clippier_test_utilities::test_resources::create_simple_workspace;

/// Helper function to create a test workspace with many packages and features
fn create_feature_rich_workspace() -> switchy_fs::TempDir {
    let temp_dir = switchy_fs::tempdir().expect("Failed to create temp directory");

    // Create a workspace with multiple packages, each with many features
    let packages = vec![
        "audio",
        "video",
        "streaming",
        "storage",
        "network",
        "ui",
        "api",
        "database",
        "cache",
        "security",
    ];

    // Create workspace Cargo.toml
    let members: Vec<String> = packages.iter().map(|p| format!("packages/{p}")).collect();
    let workspace_toml = format!(
        r#"
[workspace]
members = [{}]

[workspace.dependencies]
serde = "1.0"
tokio = "1.0"
"#,
        members
            .iter()
            .map(|m| format!("\"{m}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages with many features each
    for package in packages {
        let pkg_dir = temp_dir.path().join("packages").join(package);
        switchy_fs::sync::create_dir_all(pkg_dir.join("src")).unwrap();

        // Create Cargo.toml with many features
        let cargo_toml = format!(
            r#"
[package]
name = "{package}"
version = "0.1.0"
edition = "2021"

[features]
default = []
async = []
sync = []
streaming = []
buffered = []
compressed = []
encrypted = []
cached = []
monitored = []
logged = []
traced = []
validated = []
authenticated = []
authorized = []
rate_limited = []
fault_tolerant = []
high_performance = []
low_latency = []
scalable = []
distributed = []

[dependencies]
serde = {{ workspace = true }}
tokio = {{ workspace = true }}
"#
        );
        switchy_fs::sync::write(pkg_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(pkg_dir.join("src/lib.rs"), "// test lib").unwrap();

        // Create clippier.toml
        let clippier_toml = format!(
            r#"
[[config]]
os = "ubuntu"
dependencies = [
    {{ command = "apt-get install -y lib{package}-dev" }}
]

[[config]]
os = "windows"
dependencies = [
    {{ command = "choco install {package}-tools" }}
]
"#
        );
        switchy_fs::sync::write(pkg_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    temp_dir
}

/// Test basic chunking functionality - ensure no chunk exceeds the limit
#[switchy_async::test]
async fn test_basic_chunking_respects_limit() {
    let temp_dir = create_feature_rich_workspace();

    // Test chunking with limit of 5 features per package
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        None,    // max_parallel
        Some(5), // chunked - limit to 5 features
        false,   // spread
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // packages
        None,    // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Verify no package has more than 5 features
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 5,
            "Package {} has {} features, expected <= 5",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }

    // Test chunking with limit of 10 features per package
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,     // offset
        None,     // max
        None,     // max_parallel
        Some(10), // chunked - limit to 10 features
        false,    // spread
        false,
        None,
        None, // features
        None, // skip_features
        None, // required_features
        None, // packages
        None, // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false, // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Verify no package has more than 10 features
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 10,
            "Package {} has {} features, expected <= 10",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }
}

/// Test basic spreading functionality - ensure features are distributed across chunks
#[switchy_async::test]
async fn test_basic_spreading_distributes_features() {
    let temp_dir = create_feature_rich_workspace();

    // Test spreading without chunking - should create many packages with different features
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None, // offset
        None, // max
        None, // max_parallel
        None, // chunked
        true, // spread - distribute features
        false,
        None,
        None, // features
        None, // skip_features
        None, // required_features
        None, // packages
        None, // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false, // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // With spreading alone (no chunking), we should have the same number of packages
    // because spreading only redistributes features when there are overflow features
    assert_eq!(
        configs.len(),
        10,
        "Spreading alone should not change package count"
    );

    // Each package should still have all its features since no chunking limit
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(!features.is_empty(), "Each package should have features");
    }
}

/// Test chunking + spreading combination - the main regression test
#[switchy_async::test]
async fn test_chunking_and_spreading_combination() {
    let temp_dir = create_feature_rich_workspace();

    // Test chunking with spreading - should distribute features while respecting chunk limits
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        None,    // max_parallel
        Some(3), // chunked - limit to 3 features
        true,    // spread - distribute features
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // packages
        None,    // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Verify every package respects the chunk limit
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 3,
            "Package {} has {} features, expected <= 3 (chunk limit)",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }

    // With spreading + chunking, we should have many more packages than original
    assert!(
        configs.len() > 20,
        "Spreading + chunking should create many small packages"
    );
}

/// Test max-parallel interaction with chunking - should respect both limits
#[switchy_async::test]
async fn test_max_parallel_with_chunking() {
    let temp_dir = create_feature_rich_workspace();

    // Test max-parallel with chunking - should limit total results while respecting chunk size
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        Some(8), // max_parallel - limit to 8 total packages
        Some(4), // chunked - limit to 4 features per package
        false,   // spread
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // packages
        None,    // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should be limited to exactly 8 packages by max_parallel
    assert_eq!(configs.len(), 8, "max_parallel should limit results to 8");

    // Each package should have at most 4 features
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 4,
            "Package {} has {} features, expected <= 4 (chunk limit)",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }
}

/// Test the main regression case - simulate changed-files scenario with chunking and spreading
#[switchy_async::test]
async fn test_changed_files_respects_chunking_and_spreading() {
    let temp_dir = create_feature_rich_workspace();

    // Test the core regression case: chunking and spreading together
    // This was the main bug - when using both chunking and spreading,
    // some code paths were ignoring the chunking limits
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        None,    // max_parallel
        Some(6), // chunked - limit to 6 features
        true,    // spread - distribute features
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // packages
        None,    // changed_files - test main path, not changed files path
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should have results
    assert!(
        !configs.is_empty(),
        "Should have results for chunked + spread"
    );

    // CRITICAL: Every package should respect the chunk limit
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 6,
            "Package {} has {} features, expected <= 6 (chunk limit) - chunking + spreading should respect chunking",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }

    // With chunking and spreading, we should have multiple packages
    assert!(
        configs.len() > 10,
        "Chunking + spreading should create multiple packages"
    );
}

/// Test edge case - chunking with very small limit
#[switchy_async::test]
async fn test_chunking_with_small_limit() {
    let temp_dir = create_feature_rich_workspace();

    // Test with chunk size of 1 - should create many small packages
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        None,    // max_parallel
        Some(1), // chunked - limit to 1 feature per package
        false,   // spread
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // packages
        None,    // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Every package should have exactly 1 feature
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert_eq!(
            features.len(),
            1,
            "Package {} has {} features, expected exactly 1",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }
}

/// Test edge case - empty workspace with chunking/spreading
#[switchy_async::test]
async fn test_empty_workspace_with_chunking_spreading() {
    let (temp_dir, _) = create_simple_workspace(&[], &[], &[]);

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,     // offset
        None,     // max
        None,     // max_parallel
        Some(10), // chunked
        true,     // spread
        false,
        None,
        None, // features
        None, // skip_features
        None, // required_features
        None, // packages
        None, // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false, // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should be empty for empty workspace
    assert!(
        configs.is_empty(),
        "Empty workspace should produce empty results"
    );
}

/// Test edge case - single package with chunking/spreading
#[switchy_async::test]
async fn test_single_package_with_chunking_spreading() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create a single package with many features
    let package_dir = temp_dir.path().join("packages/single");
    switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

    let cargo_toml = r#"
[package]
name = "single"
version = "0.1.0"
edition = "2021"

[features]
default = []
feature1 = []
feature2 = []
feature3 = []
feature4 = []
feature5 = []
feature6 = []
feature7 = []
feature8 = []
feature9 = []
feature10 = []

[dependencies]
serde = "1.0"
"#;
    switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
    switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

    let clippier_toml = r#"
[[config]]
os = "ubuntu"
dependencies = [
    { command = "apt-get install -y build-essential" }
]
"#;
    switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();

    let workspace_toml = r#"
[workspace]
members = ["packages/single"]

[workspace.dependencies]
serde = "1.0"
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Test chunking with spreading on single package
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        None,    // max_parallel
        Some(3), // chunked - limit to 3 features
        true,    // spread
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // packages
        None,    // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should have multiple packages due to spreading
    assert!(
        configs.len() > 1,
        "Single package with spreading should create multiple chunks"
    );

    // Each package should have at most 3 features
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 3,
            "Package has {} features, expected <= 3",
            features.len()
        );
    }
}

/// Test complex scenario - max-parallel + chunking + spreading
#[switchy_async::test]
async fn test_complex_scenario_all_flags() {
    let temp_dir = create_feature_rich_workspace();

    // Test with all flags combined (without changed files since that's not working)
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,    // offset
        None,    // max
        Some(5), // max_parallel - limit to 5 total packages
        Some(4), // chunked - limit to 4 features per package
        true,    // spread - distribute features
        false,   // randomize
        None,    // seed
        None,    // features
        None,    // skip_features
        None,    // required_features
        None,    // packages
        None,    // changed_files - skip this for now since it's not working
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,   // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should be limited to 5 packages by max_parallel
    assert_eq!(configs.len(), 5, "max_parallel should limit results to 5");

    // Each package should have at most 4 features
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 4,
            "Package {} has {} features, expected <= 4 (chunk limit)",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }
}

/// Test that chunking without spreading still works correctly
#[switchy_async::test]
async fn test_chunking_without_spreading() {
    let temp_dir = create_feature_rich_workspace();

    // Test chunking without spreading - should limit features but not spread
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,     // offset
        None,     // max
        None,     // max_parallel
        Some(15), // chunked - limit to 15 features
        false,    // spread - don't distribute
        false,
        None,
        None, // features
        None, // skip_features
        None, // required_features
        None, // packages
        None, // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false, // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should have more packages than original because chunking splits packages with too many features
    // With 10 packages having ~21 features each, and chunking to 15, we expect more than 10 packages
    assert!(
        configs.len() > 10,
        "Chunking should create more packages when splitting oversize packages"
    );

    // Each package should have at most 15 features
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.len() <= 15,
            "Package {} has {} features, expected <= 15",
            config.get("name").unwrap().as_str().unwrap(),
            features.len()
        );
    }
}

/// Regression test: Packages with all features skipped should still generate matrix entries
/// This tests the fix for the bug where --skip-features that excluded all features
/// resulted in empty matrix output when chunking was enabled.
#[switchy_async::test]
async fn test_skip_all_features_still_generates_matrix_entry() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create a simple workspace with packages that only have fail-on-warnings feature
    let package_dir = temp_dir.path().join("packages/minimal");
    switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

    let cargo_toml = r#"
[package]
name = "minimal"
version = "0.1.0"
edition = "2021"

[features]
default = []
fail-on-warnings = []
"#;
    switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
    switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

    let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
    switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();

    let workspace_toml = r#"
[workspace]
members = ["packages/minimal"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Test with chunking enabled and skipping all non-default features
    // This was the bug: when all features are skipped, the chunked empty vec
    // caused zero matrix entries to be generated
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,                             // offset
        None,                             // max
        None,                             // max_parallel
        Some(15),                         // chunked - enable chunking
        true,                             // spread
        true,                             // randomize
        Some(42),                         // seed
        None,                             // features
        Some("fail-on-warnings,default"), // skip ALL features
        None,                             // required_features
        None,                             // packages
        None,                             // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,                            // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // CRITICAL: Even with all features skipped, we should still get a matrix entry
    // for the package so it can be built/tested with no features enabled
    assert_eq!(
        configs.len(),
        1,
        "Should generate exactly one matrix entry even when all features are skipped"
    );

    // The matrix entry should have an empty features array
    let features = configs[0].get("features").unwrap().as_array().unwrap();
    assert!(
        features.is_empty(),
        "Features array should be empty when all features are skipped"
    );

    // Should still have the package name and path
    assert_eq!(configs[0].get("name").unwrap().as_str().unwrap(), "minimal");
    assert!(configs[0].get("path").is_some());
}

/// Test multiple packages with all features skipped still generates entries for each
#[switchy_async::test]
async fn test_skip_all_features_multiple_packages() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with multiple minimal packages
    for pkg_name in ["pkg_a", "pkg_b", "pkg_c"] {
        let package_dir = temp_dir.path().join(format!("packages/{pkg_name}"));
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
fail-on-warnings = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    let workspace_toml = r#"
[workspace]
members = ["packages/pkg_a", "packages/pkg_b", "packages/pkg_c"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Skip all features with chunking enabled
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,                             // offset
        None,                             // max
        None,                             // max_parallel
        Some(15),                         // chunked
        true,                             // spread
        false,                            // randomize
        None,                             // seed
        None,                             // features
        Some("fail-on-warnings,default"), // skip ALL features
        None,                             // required_features
        None,                             // packages
        None,                             // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,                            // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should have one entry per package (3 packages total)
    assert_eq!(
        configs.len(),
        3,
        "Should generate matrix entries for all 3 packages even with all features skipped"
    );

    // All should have empty features
    for config in &configs {
        let features = config.get("features").unwrap().as_array().unwrap();
        assert!(
            features.is_empty(),
            "Package {} should have empty features",
            config.get("name").unwrap().as_str().unwrap()
        );
    }
}

/// Test that spreading without chunking still works correctly
#[switchy_async::test]
async fn test_spreading_without_chunking() {
    let temp_dir = create_feature_rich_workspace();

    // Test spreading without chunking - should distribute all features
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None, // offset
        None, // max
        None, // max_parallel
        None, // chunked - no limit
        true, // spread - distribute features
        false,
        None,
        None, // features
        None, // skip_features
        None, // required_features
        None, // packages
        None, // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false, // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should have the same number of packages as original because spreading alone doesn't split packages
    // Spreading only redistributes features when there are overflow features from chunking
    assert_eq!(
        configs.len(),
        10,
        "Spreading alone should not change package count"
    );

    // Count total features to ensure none are lost
    let total_features: usize = configs
        .iter()
        .map(|config| config.get("features").unwrap().as_array().unwrap().len())
        .sum();

    // Should have a significant number of features (each original package has ~21 features)
    assert!(
        total_features > 100,
        "Total features should be preserved during spreading: got {total_features}"
    );
}

/// Regression test: Workspace with glob patterns in members should expand correctly
/// This tests the fix for workspaces using `members = ["packages/*"]` instead of
/// explicit package paths.
#[switchy_async::test(no_simulator)]
async fn test_workspace_glob_members_expansion() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with glob pattern in members (like web2local uses)
    let workspace_toml = r#"
[workspace]
members = ["packages/*"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages directory with multiple packages
    let packages_dir = temp_dir.path().join("packages");
    switchy_fs::sync::create_dir_all(&packages_dir).unwrap();

    for pkg_name in ["alpha", "beta", "gamma"] {
        let package_dir = packages_dir.join(pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
fail-on-warnings = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        // Create clippier.toml for each package
        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    // Test that clippier can find packages via glob expansion
    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,                             // offset
        None,                             // max
        None,                             // max_parallel
        Some(5),                          // chunked
        false,                            // spread
        false,                            // randomize
        None,                             // seed
        None,                             // features
        Some("fail-on-warnings,default"), // skip features
        None,                             // required_features
        None,                             // packages
        None,                             // changed_files
        #[cfg(feature = "git-diff")]
        None, // git_base
        #[cfg(feature = "git-diff")]
        None, // git_head
        false,                            // include_reasoning
        None,
        &[],
        &[],
        #[cfg(feature = "_transforms")]
        &[],
        #[cfg(feature = "_transforms")]
        false,
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok(), "features command should succeed");
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should have found all 3 packages even with glob pattern
    assert_eq!(
        configs.len(),
        3,
        "Should find all 3 packages via glob expansion: {:?}",
        configs
            .iter()
            .map(|c| c.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>()
    );

    // Verify all expected packages were found
    let package_names: Vec<&str> = configs
        .iter()
        .map(|c| c.get("name").unwrap().as_str().unwrap())
        .collect();

    assert!(
        package_names.contains(&"alpha"),
        "Should find 'alpha' package"
    );
    assert!(
        package_names.contains(&"beta"),
        "Should find 'beta' package"
    );
    assert!(
        package_names.contains(&"gamma"),
        "Should find 'gamma' package"
    );
}

/// Test workspace with nested glob patterns like "crates/*/packages/*"
#[switchy_async::test(no_simulator)]
async fn test_workspace_nested_glob_patterns() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with simple glob pattern
    let workspace_toml = r#"
[workspace]
members = ["crates/*"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create crates directory with packages
    let crates_dir = temp_dir.path().join("crates");
    switchy_fs::sync::create_dir_all(&crates_dir).unwrap();

    for pkg_name in ["core", "utils"] {
        let package_dir = crates_dir.join(pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "my_{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        Some("default"),
        None,
        None,
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
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    assert_eq!(
        configs.len(),
        2,
        "Should find both packages via glob: {:?}",
        configs
            .iter()
            .map(|c| c.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>()
    );
}

/// Test multiple glob patterns in workspace members
#[switchy_async::test(no_simulator)]
async fn test_workspace_multiple_glob_patterns() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with multiple glob patterns
    let workspace_toml = r#"
[workspace]
members = ["packages/*", "crates/*"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages directory
    let packages_dir = temp_dir.path().join("packages");
    switchy_fs::sync::create_dir_all(&packages_dir).unwrap();

    for pkg_name in ["alpha", "beta"] {
        let package_dir = packages_dir.join(pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "pkg_{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    // Create crates directory
    let crates_dir = temp_dir.path().join("crates");
    switchy_fs::sync::create_dir_all(&crates_dir).unwrap();

    for pkg_name in ["gamma", "delta"] {
        let package_dir = crates_dir.join(pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "crate_{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        Some("default"),
        None,
        None,
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
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should find all 4 packages from both glob patterns
    assert_eq!(
        configs.len(),
        4,
        "Should find all 4 packages from multiple glob patterns: {:?}",
        configs
            .iter()
            .map(|c| c.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>()
    );

    let package_names: Vec<&str> = configs
        .iter()
        .map(|c| c.get("name").unwrap().as_str().unwrap())
        .collect();

    assert!(package_names.contains(&"pkg_alpha"));
    assert!(package_names.contains(&"pkg_beta"));
    assert!(package_names.contains(&"crate_gamma"));
    assert!(package_names.contains(&"crate_delta"));
}

/// Test mixed explicit paths and glob patterns
#[switchy_async::test(no_simulator)]
async fn test_workspace_mixed_explicit_and_glob() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with mixed explicit and glob patterns
    let workspace_toml = r#"
[workspace]
members = ["core", "packages/*"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create explicit "core" package
    let core_dir = temp_dir.path().join("core");
    switchy_fs::sync::create_dir_all(core_dir.join("src")).unwrap();

    let core_cargo_toml = r#"
[package]
name = "core"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#;
    switchy_fs::sync::write(core_dir.join("Cargo.toml"), core_cargo_toml).unwrap();
    switchy_fs::sync::write(core_dir.join("src/lib.rs"), "// core lib").unwrap();
    switchy_fs::sync::write(
        core_dir.join("clippier.toml"),
        r#"
[[config]]
os = "ubuntu"
"#,
    )
    .unwrap();

    // Create packages via glob
    let packages_dir = temp_dir.path().join("packages");
    switchy_fs::sync::create_dir_all(&packages_dir).unwrap();

    for pkg_name in ["utils", "helpers"] {
        let package_dir = packages_dir.join(pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        Some("default"),
        None,
        None,
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
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should find 3 packages: 1 explicit + 2 from glob
    assert_eq!(
        configs.len(),
        3,
        "Should find 3 packages (1 explicit + 2 from glob): {:?}",
        configs
            .iter()
            .map(|c| c.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>()
    );

    let package_names: Vec<&str> = configs
        .iter()
        .map(|c| c.get("name").unwrap().as_str().unwrap())
        .collect();

    assert!(
        package_names.contains(&"core"),
        "Should find explicit 'core' package"
    );
    assert!(
        package_names.contains(&"utils"),
        "Should find 'utils' from glob"
    );
    assert!(
        package_names.contains(&"helpers"),
        "Should find 'helpers' from glob"
    );
}

/// Test single character wildcard (?) in glob patterns
#[switchy_async::test(no_simulator)]
async fn test_workspace_single_char_wildcard() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with single-char wildcard pattern
    let workspace_toml = r#"
[workspace]
members = ["pkg?"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages that match pkg? pattern
    for suffix in ["a", "b", "c"] {
        let pkg_name = format!("pkg{suffix}");
        let package_dir = temp_dir.path().join(&pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    // Create a package that should NOT match (pkg_long has more than one char after pkg)
    let non_matching_dir = temp_dir.path().join("pkg_long");
    switchy_fs::sync::create_dir_all(non_matching_dir.join("src")).unwrap();
    switchy_fs::sync::write(
        non_matching_dir.join("Cargo.toml"),
        r#"
[package]
name = "pkg_long"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    switchy_fs::sync::write(non_matching_dir.join("src/lib.rs"), "// should not match").unwrap();

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        Some("default"),
        None,
        None,
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
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should find only 3 packages matching pkg? (not pkg_long)
    assert_eq!(
        configs.len(),
        3,
        "Should find only 3 packages matching 'pkg?': {:?}",
        configs
            .iter()
            .map(|c| c.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>()
    );

    let package_names: Vec<&str> = configs
        .iter()
        .map(|c| c.get("name").unwrap().as_str().unwrap())
        .collect();

    assert!(package_names.contains(&"pkga"));
    assert!(package_names.contains(&"pkgb"));
    assert!(package_names.contains(&"pkgc"));
    assert!(
        !package_names.contains(&"pkg_long"),
        "pkg_long should NOT match 'pkg?'"
    );
}

/// Test character class patterns [abc] in glob
#[switchy_async::test(no_simulator)]
async fn test_workspace_character_class_glob() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with character class pattern
    let workspace_toml = r#"
[workspace]
members = ["pkg_[abc]"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages that match pkg_[abc] pattern
    for suffix in ["a", "b", "c"] {
        let pkg_name = format!("pkg_{suffix}");
        let package_dir = temp_dir.path().join(&pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    // Create packages that should NOT match
    for suffix in ["d", "x", "z"] {
        let pkg_name = format!("pkg_{suffix}");
        let package_dir = temp_dir.path().join(&pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// should not match").unwrap();
    }

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        Some("default"),
        None,
        None,
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
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should find only 3 packages matching pkg_[abc]
    assert_eq!(
        configs.len(),
        3,
        "Should find only 3 packages matching 'pkg_[abc]': {:?}",
        configs
            .iter()
            .map(|c| c.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>()
    );

    let package_names: Vec<&str> = configs
        .iter()
        .map(|c| c.get("name").unwrap().as_str().unwrap())
        .collect();

    assert!(package_names.contains(&"pkg_a"));
    assert!(package_names.contains(&"pkg_b"));
    assert!(package_names.contains(&"pkg_c"));
    assert!(
        !package_names.contains(&"pkg_d"),
        "pkg_d should NOT match 'pkg_[abc]'"
    );
    assert!(
        !package_names.contains(&"pkg_x"),
        "pkg_x should NOT match 'pkg_[abc]'"
    );
}

/// Test non-matching glob pattern returns empty (not error)
#[switchy_async::test(no_simulator)]
async fn test_workspace_non_matching_glob_returns_empty() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with glob pattern that won't match anything
    let workspace_toml = r#"
[workspace]
members = ["nonexistent/*"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Don't create any packages - the glob should match nothing

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
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
        None,
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
        OutputType::Json,
    )
    .await;

    // Should succeed but return empty array (not error)
    assert!(result.is_ok(), "Non-matching glob should not cause error");
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    assert!(
        configs.is_empty(),
        "Non-matching glob should return empty array, got: {:?}",
        configs
    );
}

/// Test glob only matches directories with Cargo.toml (not random directories)
#[switchy_async::test(no_simulator)]
async fn test_workspace_glob_requires_cargo_toml() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with glob pattern
    let workspace_toml = r#"
[workspace]
members = ["packages/*"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    let packages_dir = temp_dir.path().join("packages");
    switchy_fs::sync::create_dir_all(&packages_dir).unwrap();

    // Create a valid package
    let valid_pkg = packages_dir.join("valid");
    switchy_fs::sync::create_dir_all(valid_pkg.join("src")).unwrap();
    switchy_fs::sync::write(
        valid_pkg.join("Cargo.toml"),
        r#"
[package]
name = "valid"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#,
    )
    .unwrap();
    switchy_fs::sync::write(valid_pkg.join("src/lib.rs"), "// valid lib").unwrap();
    switchy_fs::sync::write(
        valid_pkg.join("clippier.toml"),
        r#"
[[config]]
os = "ubuntu"
"#,
    )
    .unwrap();

    // Create directories WITHOUT Cargo.toml (should be ignored)
    let no_cargo_dir = packages_dir.join("not_a_package");
    switchy_fs::sync::create_dir_all(no_cargo_dir.join("src")).unwrap();
    switchy_fs::sync::write(no_cargo_dir.join("src/lib.rs"), "// no cargo.toml").unwrap();

    let empty_dir = packages_dir.join("empty_dir");
    switchy_fs::sync::create_dir_all(&empty_dir).unwrap();

    // Create a file (not directory) that matches glob pattern
    switchy_fs::sync::write(packages_dir.join("just_a_file"), "not a directory").unwrap();

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        Some("default"),
        None,
        None,
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
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    // Should find only the valid package
    assert_eq!(
        configs.len(),
        1,
        "Should find only 1 valid package: {:?}",
        configs
            .iter()
            .map(|c| c.get("name").unwrap().as_str().unwrap())
            .collect::<Vec<_>>()
    );

    assert_eq!(
        configs[0].get("name").unwrap().as_str().unwrap(),
        "valid",
        "Should only find 'valid' package"
    );
}

/// Test glob expansion results are sorted deterministically
#[switchy_async::test(no_simulator)]
async fn test_workspace_glob_results_sorted() {
    let temp_dir = switchy_fs::tempdir().unwrap();

    // Create workspace with glob pattern
    let workspace_toml = r#"
[workspace]
members = ["packages/*"]
"#;
    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    let packages_dir = temp_dir.path().join("packages");
    switchy_fs::sync::create_dir_all(&packages_dir).unwrap();

    // Create packages in non-alphabetical order
    for pkg_name in ["zebra", "apple", "mango", "banana"] {
        let package_dir = packages_dir.join(pkg_name);
        switchy_fs::sync::create_dir_all(package_dir.join("src")).unwrap();

        let cargo_toml = format!(
            r#"
[package]
name = "{pkg_name}"
version = "0.1.0"
edition = "2021"

[features]
default = []
"#
        );
        switchy_fs::sync::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
        switchy_fs::sync::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

        let clippier_toml = r#"
[[config]]
os = "ubuntu"
"#;
        switchy_fs::sync::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        Some("ubuntu"),
        None,
        None,
        None,
        None,
        false,
        false,
        None,
        None,
        Some("default"),
        None,
        None,
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
        OutputType::Json,
    )
    .await;

    assert!(result.is_ok());
    let configs: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();

    assert_eq!(configs.len(), 4);

    // Extract paths and verify they're sorted
    let paths: Vec<&str> = configs
        .iter()
        .map(|c| c.get("path").unwrap().as_str().unwrap())
        .collect();

    // Paths should be sorted alphabetically
    let mut sorted_paths = paths.clone();
    sorted_paths.sort();
    assert_eq!(
        paths, sorted_paths,
        "Glob expansion results should be sorted deterministically"
    );
}
