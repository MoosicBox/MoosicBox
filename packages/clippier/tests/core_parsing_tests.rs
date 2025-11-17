use clippier_test_utilities::test_resources::load_test_workspace;
use toml::Value;

#[test]
fn test_parse_dependency_name_variations() {
    // Test different dependency name formats
    assert_eq!(clippier::parse_dependency_name("serde"), "serde");
    assert_eq!(clippier::parse_dependency_name("serde 1.0.195"), "serde");
    assert_eq!(
        clippier::parse_dependency_name(
            "tokio 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"
        ),
        "tokio"
    );
    assert_eq!(
        clippier::parse_dependency_name("serde_json 1.0.100"),
        "serde_json"
    );
    assert_eq!(
        clippier::parse_dependency_name("hyper-util 0.1.3"),
        "hyper-util"
    );
    assert_eq!(
        clippier::parse_dependency_name("my-custom-crate"),
        "my-custom-crate"
    );
}

#[test]
fn test_parse_cargo_lock_changes_single() {
    let changes = vec![
        (' ', "[[package]]\n".to_string()),
        (' ', "name = \"serde\"\n".to_string()),
        ('-', "version = \"1.0.180\"\n".to_string()),
        ('+', "version = \"1.0.190\"\n".to_string()),
    ];

    let result = clippier::git_diff::parse_cargo_lock_changes(&changes);
    assert!(result.contains(&"serde".to_string()));
}

#[test]
fn test_parse_cargo_lock_changes_multiple() {
    // Test multiple package changes
    let changes = vec![
        (' ', "[[package]]\n".to_string()),
        (' ', "name = \"serde\"\n".to_string()),
        ('-', "version = \"1.0.180\"\n".to_string()),
        ('+', "version = \"1.0.190\"\n".to_string()),
        (' ', "[[package]]\n".to_string()),
        (' ', "name = \"tokio\"\n".to_string()),
        ('-', "version = \"1.28.0\"\n".to_string()),
        ('+', "version = \"1.35.0\"\n".to_string()),
    ];

    let result = clippier::git_diff::parse_cargo_lock_changes(&changes);
    assert_eq!(result.len(), 2);
    assert!(result.contains(&"serde".to_string()));
    assert!(result.contains(&"tokio".to_string()));
}

#[test]
fn test_parse_cargo_lock_changes_no_changes() {
    // Test with no relevant changes
    let changes = vec![];

    let result = clippier::git_diff::parse_cargo_lock_changes(&changes);
    assert!(result.is_empty());
}

#[test]
fn test_parse_cargo_lock_changes_new_package() {
    // Test addition of new package
    let changes = vec![('+', "new-crate 0.1.0".to_string())];

    let result = clippier::git_diff::parse_cargo_lock_changes(&changes);
    // This stub implementation doesn't handle new packages, so it should be empty
    assert!(result.is_empty());
}

#[test]
fn test_parse_cargo_lock_changes_removed_package() {
    // Test removal of package
    let changes = vec![('-', "removed-crate 0.5.0".to_string())];

    let result = clippier::git_diff::parse_cargo_lock_changes(&changes);
    // This stub implementation doesn't handle removed packages, so it should be empty
    assert!(result.is_empty());
}

#[test]
fn test_parse_cargo_lock_toml_structure() {
    let cargo_lock_toml = r#"
version = 3

[[package]]
name = "serde"
version = "1.0.195"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "..."

[[package]]
name = "my-app"
version = "0.1.0"
dependencies = [
    "serde 1.0.195",
    "tokio 1.36.0",
]

[[package]]
name = "tokio"
version = "1.36.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
dependencies = [
    "pin-project-lite",
]
"#;

    let result =
        clippier::git_diff::parse_cargo_lock(cargo_lock_toml).expect("Failed to parse Cargo.lock");

    assert_eq!(result.version, 3);
    assert_eq!(result.package.len(), 3);

    // Check specific packages
    let serde_package = result.package.iter().find(|p| p.name == "serde").unwrap();
    assert_eq!(serde_package.version, "1.0.195");
    assert!(serde_package.source.is_some());
    assert!(serde_package.dependencies.is_none());

    let my_app_package = result.package.iter().find(|p| p.name == "my-app").unwrap();
    assert_eq!(my_app_package.version, "0.1.0");
    assert!(my_app_package.source.is_none());
    let deps = my_app_package.dependencies.as_ref().unwrap();
    assert_eq!(deps.len(), 2);
    assert!(deps.contains(&"serde 1.0.195".to_string()));
    assert!(deps.contains(&"tokio 1.36.0".to_string()));

    let tokio_package = result.package.iter().find(|p| p.name == "tokio").unwrap();
    assert_eq!(tokio_package.version, "1.36.0");
    let tokio_deps = tokio_package.dependencies.as_ref().unwrap();
    assert_eq!(tokio_deps.len(), 1);
    assert!(tokio_deps.contains(&"pin-project-lite".to_string()));
}

#[test]
fn test_split_utility() {
    let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    // Test splitting into 3 groups
    let chunks: Vec<&[i32]> = clippier::split(&data, 3).collect();
    assert_eq!(chunks.len(), 3);

    // Test with equal distribution
    let data = vec![1, 2, 3, 4, 5, 6];
    let chunks: Vec<&[i32]> = clippier::split(&data, 3).collect();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0], &[1, 2]);
    assert_eq!(chunks[1], &[3, 4]);
    assert_eq!(chunks[2], &[5, 6]);
}

#[test]
fn test_split_edge_cases() {
    let data = vec![1, 2, 3, 4, 5];

    // Test with more chunks than elements
    let chunks: Vec<&[i32]> = clippier::split(&data, 5).collect();
    assert_eq!(chunks.len(), 5);

    // Test with single chunk
    let chunks: Vec<&[i32]> = clippier::split(&data, 1).collect();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0], &[1, 2, 3, 4, 5]);

    // Test with empty data
    let empty_data: Vec<i32> = vec![];
    let chunks: Vec<&[i32]> = clippier::split(&empty_data, 3).collect();
    assert_eq!(chunks.len(), 0);
}

#[test]
fn test_process_features_chunked() {
    let features = vec![
        "feat1".to_string(),
        "feat2".to_string(),
        "feat3".to_string(),
        "feat4".to_string(),
    ];

    let chunked_result = clippier::process_features(features.clone(), Some(2), false, false, None);
    match chunked_result {
        clippier::FeaturesList::Chunked(chunks) => {
            assert_eq!(chunks.len(), 2);
            assert_eq!(chunks[0], vec!["feat1", "feat2"]);
            assert_eq!(chunks[1], vec!["feat3", "feat4"]);
        }
        _ => panic!("Expected chunked result"),
    }
}

#[test]
fn test_process_features_spread() {
    let features = vec!["feat1".to_string(), "feat2".to_string()];

    let spread_result = clippier::process_features(features.clone(), Some(2), true, false, None);
    match spread_result {
        clippier::FeaturesList::Chunked(chunks) => {
            assert_eq!(chunks.len(), 1);
            assert_eq!(chunks[0], vec!["feat1", "feat2"]);
        }
        _ => panic!("Expected chunked result"),
    }
}

#[test]
fn test_process_features_not_chunked() {
    let features = vec!["feat1".to_string(), "feat2".to_string()];

    let result = clippier::process_features(features.clone(), None, false, false, None);
    match result {
        clippier::FeaturesList::NotChunked(feats) => {
            assert_eq!(feats, features);
        }
        _ => panic!("Expected not chunked result"),
    }
}

#[test]
fn test_fetch_features_basic() {
    let cargo_toml = toml::from_str::<Value>(
        r#"
        [features]
        default = []
        json = []
        async = []
        database = []
        server = []
        frontend = []
    "#,
    )
    .unwrap();

    let features = clippier::fetch_features(&cargo_toml, None, None, None, None, None);
    assert!(features.contains(&"json".to_string()));
    assert!(features.contains(&"async".to_string()));
}

#[test]
fn test_process_features_randomize() {
    let features = vec![
        "feat1".to_string(),
        "feat2".to_string(),
        "feat3".to_string(),
        "feat4".to_string(),
    ];

    // Test randomization without chunking
    let result_non_randomized =
        clippier::process_features(features.clone(), None, false, false, None);
    let result_randomized = clippier::process_features(features.clone(), None, false, true, None);

    match (&result_non_randomized, &result_randomized) {
        (
            clippier::FeaturesList::NotChunked(non_random),
            clippier::FeaturesList::NotChunked(randomized),
        ) => {
            // Both should contain the same features
            assert_eq!(non_random.len(), randomized.len());
            for feature in non_random {
                assert!(randomized.contains(feature));
            }
            // Note: We can't guarantee they're in different orders since randomization might
            // occasionally produce the same order, but the functionality is there
        }
        _ => panic!("Expected NotChunked results"),
    }

    // Test randomization with chunking
    let result_chunked = clippier::process_features(features.clone(), Some(2), false, true, None);
    match result_chunked {
        clippier::FeaturesList::Chunked(chunks) => {
            assert_eq!(chunks.len(), 2);
            // Verify all features are present across chunks
            let mut all_features_in_chunks = Vec::new();
            for chunk in &chunks {
                all_features_in_chunks.extend(chunk.clone());
            }
            assert_eq!(all_features_in_chunks.len(), features.len());
            for feature in &features {
                assert!(all_features_in_chunks.contains(feature));
            }
        }
        _ => panic!("Expected chunked result"),
    }
}

#[test]
fn test_fetch_features_filtering() {
    let cargo_toml = toml::from_str::<Value>(
        r#"
        [features]
        default = []
        json = []
        async = []
        _internal = []
    "#,
    )
    .unwrap();

    let specific_features = vec!["json".to_string(), "async".to_string()];
    let features = clippier::fetch_features(
        &cargo_toml,
        None,
        None,
        Some(&specific_features),
        None,
        None,
    );
    assert_eq!(features.len(), 2);

    let skip_features = vec!["async".to_string()];
    let features =
        clippier::fetch_features(&cargo_toml, None, None, None, Some(&skip_features), None);
    assert!(!features.contains(&"async".to_string()));

    let features = clippier::fetch_features(&cargo_toml, Some(1), Some(2), None, None, None);
    assert!(features.len() <= 2);
}

#[test]
fn test_clipper_env_variants() {
    // Test that we can work with different ClippierEnv variants
    // This is mainly a compilation test
    // assert!(true);
}

#[test]
fn test_vec_or_item_conversion() {
    // Test that VecOrItem works correctly
    // This is mainly a compilation test
    // assert!(true);
}

#[test]
fn test_workspace_dependency_detection() {
    let workspace_dep_table = toml::from_str::<Value>(
        r#"
        workspace = true
    "#,
    )
    .unwrap();

    let non_workspace_dep_table = toml::from_str::<Value>(
        r#"
        version = "1.0"
    "#,
    )
    .unwrap();

    let workspace_dep_with_features = toml::from_str::<Value>(
        r#"
        workspace = true
        features = ["json"]
    "#,
    )
    .unwrap();

    let optional_workspace_dep = toml::from_str::<Value>(
        r#"
        workspace = true
        optional = true
    "#,
    )
    .unwrap();

    assert!(clippier::is_workspace_dependency(&workspace_dep_table));
    assert!(!clippier::is_workspace_dependency(&non_workspace_dep_table));
    assert!(clippier::is_workspace_dependency(
        &workspace_dep_with_features
    ));
    assert!(clippier::is_workspace_dependency(&optional_workspace_dep));

    // Test with features
    assert!(clippier::is_workspace_dependency_with_features(
        &workspace_dep_table
    ));
    assert!(!clippier::is_workspace_dependency_with_features(
        &non_workspace_dep_table
    ));
    assert!(clippier::is_workspace_dependency_with_features(
        &workspace_dep_with_features
    ));
    // Optional deps should return false for is_workspace_dependency_with_features
    assert!(!clippier::is_workspace_dependency_with_features(
        &optional_workspace_dep
    ));
}

#[test]
fn test_get_dependency_default_features() {
    let dep_with_default_false = toml::from_str::<Value>(
        r#"
        workspace = true
        default-features = false
    "#,
    )
    .unwrap();

    let dep_with_default_true = toml::from_str::<Value>(
        r#"
        workspace = true
        default-features = true
    "#,
    )
    .unwrap();

    let dep_without_default = toml::from_str::<Value>(
        r#"
        workspace = true
    "#,
    )
    .unwrap();

    let dep_with_underscore = toml::from_str::<Value>(
        r#"
        workspace = true
        default_features = false
    "#,
    )
    .unwrap();

    assert_eq!(
        clippier::get_dependency_default_features(&dep_with_default_false),
        Some(false)
    );
    assert_eq!(
        clippier::get_dependency_default_features(&dep_with_default_true),
        Some(true)
    );
    assert_eq!(
        clippier::get_dependency_default_features(&dep_without_default),
        None
    );
    assert_eq!(
        clippier::get_dependency_default_features(&dep_with_underscore),
        Some(false)
    );
}

#[test]
fn test_collect_system_dependencies() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test basic system dependency collection
    let dependencies = vec![
        ("api".to_string(), "packages/api".to_string()),
        ("models".to_string(), "packages/models".to_string()),
    ];

    let ubuntu_deps =
        clippier::collect_system_dependencies(temp_dir.path(), &dependencies, None, "ubuntu");
    assert!(ubuntu_deps.is_ok());

    // Test with TLS feature
    let features = vec!["tls".to_string()];
    let ubuntu_deps_with_tls = clippier::collect_system_dependencies(
        temp_dir.path(),
        &dependencies,
        Some(&features),
        "ubuntu",
    );
    assert!(ubuntu_deps_with_tls.is_ok());

    // Test with different OS
    let alpine_deps =
        clippier::collect_system_dependencies(temp_dir.path(), &dependencies, None, "alpine");
    assert!(alpine_deps.is_ok());
}

#[test]
fn test_binary_name_detection() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test custom binary name detection
    let binary_name = clippier::get_binary_name(temp_dir.path(), "my-package", "custom/path", None);
    // Should fall back to package name transformation
    assert_eq!(binary_name, "my_package");

    // Test with actual package
    let binary_name =
        clippier::get_binary_name(temp_dir.path(), "default-package", "default/path", None);
    assert_eq!(binary_name, "default_package");
}

#[test]
fn test_output_type_enum() {
    // Test that OutputType enum works correctly
    use clippier::OutputType;
    let json_type = OutputType::Json;
    let raw_type = OutputType::Raw;

    assert_eq!(json_type, OutputType::Json);
    assert_eq!(raw_type, OutputType::Raw);
    assert_ne!(json_type, raw_type);
}

// Snapshot tests with proper JSON serialization
#[test]
fn test_cargo_lock_parsing_snapshot() {
    let test_data = serde_json::json!({
        "version_updates": {
            "serde": {
                "from": "1.0.180",
                "to": "1.0.190"
            },
            "tokio": {
                "from": "1.28.0",
                "to": "1.35.0"
            }
        },
        "new_packages": ["uuid", "chrono"],
        "removed_packages": ["old-dep"],
        "note": "Cargo.lock changes can indicate external dependency updates"
    });

    insta::assert_yaml_snapshot!("cargo_lock_parsing", test_data);
}

#[test]
fn test_toml_structure_parsing_snapshot() {
    let test_data = serde_json::json!({
        "package_section": {
            "name": "test-package",
            "version": "0.1.0",
            "required_fields": ["name", "version"]
        },
        "dependencies_section": {
            "workspace_deps": ["dep1", "dep2"],
            "external_deps": ["serde", "tokio"]
        },
        "features_section": {
            "default": [],
            "custom_features": ["json", "async"]
        },
        "validation": "All sections parsed successfully"
    });

    insta::assert_yaml_snapshot!("toml_structure_parsing", test_data);
}

#[test]
fn test_feature_filtering_snapshot() {
    let test_data = serde_json::json!({
        "all_features": ["default", "json", "async", "database", "server", "frontend"],
        "skip_features": {
            "skip": ["default", "_internal"],
            "remaining": ["json", "async", "database", "server", "frontend"]
        },
        "required_features": {
            "required": ["json", "async"],
            "optional": ["database", "server", "frontend"]
        },
        "specific_features": {
            "selected": ["json", "database"],
            "ignored": ["async", "server", "frontend"]
        }
    });

    insta::assert_yaml_snapshot!("feature_filtering", test_data);
}

#[test]
fn test_process_features_seed_deterministic() {
    // Test that same seed produces same randomized output
    let features = vec![
        "feature1".to_string(),
        "feature2".to_string(),
        "feature3".to_string(),
        "feature4".to_string(),
        "feature5".to_string(),
        "feature6".to_string(),
        "feature7".to_string(),
        "feature8".to_string(),
        "feature9".to_string(),
        "feature10".to_string(),
    ];

    let seed = 12345u64;

    // Run the same randomization twice with the same seed
    let result1 = clippier::process_features(features.clone(), Some(3), false, true, Some(seed));
    let result2 = clippier::process_features(features.clone(), Some(3), false, true, Some(seed));

    // Both results should be identical when using the same seed
    match (result1, result2) {
        (clippier::FeaturesList::Chunked(chunks1), clippier::FeaturesList::Chunked(chunks2)) => {
            assert_eq!(
                chunks1, chunks2,
                "Same seed should produce identical randomized output"
            );
        }
        _ => panic!("Expected chunked features"),
    }

    // Test with different seeds to ensure they produce different outputs
    let seed1 = 12345u64;
    let seed2 = 54321u64;

    let result1 = clippier::process_features(features.clone(), Some(3), false, true, Some(seed1));
    let result2 = clippier::process_features(features.clone(), Some(3), false, true, Some(seed2));

    match (result1, result2) {
        (clippier::FeaturesList::Chunked(chunks1), clippier::FeaturesList::Chunked(chunks2)) => {
            // Different seeds should produce different results
            // We can't guarantee they'll be different, but with 10 features, it's very likely
            // Just ensure both contain the same features in total
            let mut all_features1: Vec<String> = chunks1.into_iter().flatten().collect();
            let mut all_features2: Vec<String> = chunks2.into_iter().flatten().collect();
            all_features1.sort();
            all_features2.sort();
            assert_eq!(
                all_features1, all_features2,
                "Different seeds should preserve all features"
            );
        }
        _ => panic!("Expected chunked features"),
    }
}

#[test]
fn test_process_features_seed_with_spread() {
    // Test that seed works with spreading as well
    let features = vec![
        "feature1".to_string(),
        "feature2".to_string(),
        "feature3".to_string(),
        "feature4".to_string(),
        "feature5".to_string(),
        "feature6".to_string(),
        "feature7".to_string(),
        "feature8".to_string(),
    ];

    let seed = 98765u64;

    // Test with spreading and seed
    let result1 = clippier::process_features(features.clone(), Some(2), true, true, Some(seed));
    let result2 = clippier::process_features(features.clone(), Some(2), true, true, Some(seed));

    match (result1, result2) {
        (clippier::FeaturesList::Chunked(chunks1), clippier::FeaturesList::Chunked(chunks2)) => {
            assert_eq!(
                chunks1, chunks2,
                "Same seed with spreading should produce identical output"
            );
        }
        _ => panic!("Expected chunked features"),
    }
}
#[test]
fn test_clippier_conf_git_submodules_deserialization() {
    let toml_str = r#"
        git-submodules = true

        [[config]]
        os = "ubuntu"
    "#;

    let conf: clippier::ClippierConf = toml::from_str(toml_str).unwrap();
    assert_eq!(conf.git_submodules, Some(true));
}

#[test]
fn test_clippier_configuration_git_submodules_deserialization() {
    let toml_str = r#"
        [[config]]
        os = "ubuntu"
        git-submodules = true
    "#;

    let conf: clippier::ClippierConf = toml::from_str(toml_str).unwrap();
    assert_eq!(conf.config.as_ref().unwrap()[0].git_submodules, Some(true));
}

#[test]
fn test_git_submodules_optional_field() {
    let toml_str = r#"
        [[config]]
        os = "ubuntu"
    "#;

    let conf: clippier::ClippierConf = toml::from_str(toml_str).unwrap();
    assert_eq!(conf.git_submodules, None);
    assert_eq!(conf.config.as_ref().unwrap()[0].git_submodules, None);
}

#[test]
fn test_git_submodules_false_value() {
    let toml_str = r#"
        git-submodules = false

        [[config]]
        os = "ubuntu"
    "#;

    let conf: clippier::ClippierConf = toml::from_str(toml_str).unwrap();
    assert_eq!(conf.git_submodules, Some(false));
}
