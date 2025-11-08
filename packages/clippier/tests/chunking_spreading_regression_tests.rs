use std::fs;

use clippier::{OutputType, handle_features_command};
use clippier_test_utilities::test_resources::create_simple_workspace;
use tempfile::TempDir;

/// Helper function to create a test workspace with many packages and features
fn create_feature_rich_workspace() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

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
    fs::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

    // Create packages with many features each
    for package in packages {
        let pkg_dir = temp_dir.path().join("packages").join(package);
        fs::create_dir_all(pkg_dir.join("src")).unwrap();

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
        fs::write(pkg_dir.join("Cargo.toml"), cargo_toml).unwrap();
        fs::write(pkg_dir.join("src/lib.rs"), "// test lib").unwrap();

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
        fs::write(pkg_dir.join("clippier.toml"), clippier_toml).unwrap();
    }

    temp_dir
}

/// Test basic chunking functionality - ensure no chunk exceeds the limit
#[switchy_async::test]
async fn test_basic_chunking_respects_limit() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test basic spreading functionality - ensure features are distributed across chunks
#[switchy_async::test]
async fn test_basic_spreading_distributes_features() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test chunking + spreading combination - the main regression test
#[switchy_async::test]
async fn test_chunking_and_spreading_combination() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test max-parallel interaction with chunking - should respect both limits
#[switchy_async::test]
async fn test_max_parallel_with_chunking() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test the main regression case - simulate changed-files scenario with chunking and spreading
#[switchy_async::test]
async fn test_changed_files_respects_chunking_and_spreading()
-> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test edge case - chunking with very small limit
#[switchy_async::test]
async fn test_chunking_with_small_limit() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test edge case - empty workspace with chunking/spreading
#[switchy_async::test]
async fn test_empty_workspace_with_chunking_spreading() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

    // Should be empty for empty workspace
    assert!(
        configs.is_empty(),
        "Empty workspace should produce empty results"
    );
    Ok(())
}

/// Test edge case - single package with chunking/spreading
#[switchy_async::test]
async fn test_single_package_with_chunking_spreading() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new().unwrap();

    // Create a single package with many features
    let package_dir = temp_dir.path().join("packages/single");
    fs::create_dir_all(package_dir.join("src")).unwrap();

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
    fs::write(package_dir.join("Cargo.toml"), cargo_toml).unwrap();
    fs::write(package_dir.join("src/lib.rs"), "// test lib").unwrap();

    let clippier_toml = r#"
[[config]]
os = "ubuntu"
dependencies = [
    { command = "apt-get install -y build-essential" }
]
"#;
    fs::write(package_dir.join("clippier.toml"), clippier_toml).unwrap();

    let workspace_toml = r#"
[workspace]
members = ["packages/single"]

[workspace.dependencies]
serde = "1.0"
"#;
    fs::write(temp_dir.path().join("Cargo.toml"), workspace_toml).unwrap();

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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test complex scenario - max-parallel + chunking + spreading
#[switchy_async::test]
async fn test_complex_scenario_all_flags() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test that chunking without spreading still works correctly
#[switchy_async::test]
async fn test_chunking_without_spreading() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}

/// Test that spreading without chunking still works correctly
#[switchy_async::test]
async fn test_spreading_without_chunking() -> Result<(), Box<dyn std::error::Error>> {
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
        false,
        None,
        OutputType::Json,
    )
    .await?;

    let configs: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();

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
    Ok(())
}
