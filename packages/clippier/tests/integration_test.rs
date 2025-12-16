#[cfg(feature = "git-diff")]
use clippier::git_diff::{
    build_external_dependency_map, find_packages_affected_by_external_deps,
    find_transitively_affected_external_deps,
};

use clippier::test_utils::test_resources::{load_cargo_lock_for_git_diff, load_test_workspace};

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_build_external_dependency_map() {
    let (temp_dir, workspace_members) = load_test_workspace("basic");

    let external_dep_map = build_external_dependency_map(temp_dir.path(), &workspace_members)
        .expect("Failed to build external dependency map");

    insta::assert_debug_snapshot!(external_dep_map, @r###"
    {
        "reqwest": [
            "api",
            "client",
        ],
        "serde": [
            "api",
            "models",
        ],
        "tokio": [
            "api",
        ],
    }
    "###);
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_end_to_end_external_dependency_analysis() {
    let (temp_dir, workspace_members) = load_test_workspace("basic");

    // Load the comprehensive Cargo.lock from test resources
    let cargo_lock = load_cargo_lock_for_git_diff("basic", "comprehensive");

    // Test scenario: pin-project-lite changes
    let directly_changed_deps = vec!["pin-project-lite".to_string()];
    let all_affected_external_deps =
        find_transitively_affected_external_deps(&cargo_lock, &directly_changed_deps);

    insta::assert_debug_snapshot!(all_affected_external_deps, @r###"
    [
        "api",
        "client",
        "pin-project-lite",
        "reqwest",
        "tokio",
    ]
    "###);

    // Build external dependency map
    let external_dep_map = build_external_dependency_map(temp_dir.path(), &workspace_members)
        .expect("Failed to build external dependency map");

    // Find workspace packages affected by the external dependency changes
    let affected_workspace_packages =
        find_packages_affected_by_external_deps(&external_dep_map, &all_affected_external_deps);

    insta::assert_debug_snapshot!(affected_workspace_packages, @r###"
    [
        "api",
        "client",
    ]
    "###);
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_deep_transitive_dependency_change() {
    let (temp_dir, workspace_members) = load_test_workspace("basic");

    // Load the complex deep dependencies Cargo.lock from test resources
    let cargo_lock = load_cargo_lock_for_git_diff("deep-deps", "complex");

    // Test scenario: openssl-sys changes (affects multiple levels)
    let directly_changed_deps = vec!["openssl-sys".to_string()];
    let all_affected_external_deps =
        find_transitively_affected_external_deps(&cargo_lock, &directly_changed_deps);

    insta::assert_debug_snapshot!(all_affected_external_deps, @r###"
    [
        "my-http-client",
        "native-tls",
        "openssl",
        "openssl-sys",
        "reqwest",
    ]
    "###);

    // Build external dependency map
    let external_dep_map = build_external_dependency_map(temp_dir.path(), &workspace_members)
        .expect("Failed to build external dependency map");

    // Find workspace packages affected by the external dependency changes
    let affected_workspace_packages =
        find_packages_affected_by_external_deps(&external_dep_map, &all_affected_external_deps);

    // In this case, the basic workspace doesn't have my-http-client,
    // but it would be affected if we had a workspace with that package
    insta::assert_debug_snapshot!(affected_workspace_packages, @r###"
    [
        "api",
        "client",
    ]
    "###);
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_multiple_level_dependency_changes() {
    let (temp_dir, workspace_members) = load_test_workspace("basic");

    // Load comprehensive Cargo.lock that includes multiple-level dependencies
    let cargo_lock = load_cargo_lock_for_git_diff("basic", "comprehensive");

    // Test scenario: multiple dependencies change at once
    let directly_changed_deps = vec!["serde".to_string(), "tokio".to_string()];
    let all_affected_external_deps =
        find_transitively_affected_external_deps(&cargo_lock, &directly_changed_deps);

    insta::assert_debug_snapshot!(all_affected_external_deps, @r###"
    [
        "api",
        "client",
        "models",
        "reqwest",
        "serde",
        "serde_json",
        "tokio",
    ]
    "###);

    // Build external dependency map
    let external_dep_map = build_external_dependency_map(temp_dir.path(), &workspace_members)
        .expect("Failed to build external dependency map");

    // Find workspace packages affected by the external dependency changes
    let affected_workspace_packages =
        find_packages_affected_by_external_deps(&external_dep_map, &all_affected_external_deps);

    insta::assert_debug_snapshot!(affected_workspace_packages, @r###"
    [
        "api",
        "client",
        "models",
    ]
    "###);
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_no_transitive_impact() {
    let (temp_dir, workspace_members) = load_test_workspace("basic");

    // Create a test scenario where a dependency changes but doesn't affect workspace packages
    let cargo_lock = load_cargo_lock_for_git_diff("basic", "simple");

    // Test scenario: only external dependencies change, no workspace impact
    let directly_changed_deps = vec!["some-unrelated-crate".to_string()];
    let all_affected_external_deps =
        find_transitively_affected_external_deps(&cargo_lock, &directly_changed_deps);

    // Should only include the directly changed dependency since it doesn't exist in our cargo lock
    insta::assert_debug_snapshot!(all_affected_external_deps, @r###"
    [
        "some-unrelated-crate",
    ]
    "###);

    // Build external dependency map
    let external_dep_map = build_external_dependency_map(temp_dir.path(), &workspace_members)
        .expect("Failed to build external dependency map");

    // Find workspace packages affected by the external dependency changes
    let affected_workspace_packages =
        find_packages_affected_by_external_deps(&external_dep_map, &all_affected_external_deps);

    // Should be empty since the changed external dependency doesn't affect any workspace packages
    insta::assert_debug_snapshot!(affected_workspace_packages, @"[]");
}

#[switchy_async::test]
async fn test_git_submodules_with_chunking_and_spreading() {
    use clippier::{OutputType, handle_features_command};
    use clippier_test_utilities::test_resources::load_test_workspace;

    let (temp_dir, _) = load_test_workspace("git-submodules");

    let result = handle_features_command(
        temp_dir.path().to_str().unwrap(),
        None,
        None,
        None,
        Some(2),
        None,
        true,
        true,
        Some(12345),
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
        None, // workspace_type filter
        OutputType::Json,
    )
    .await
    .unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    let with_submodules_entries: Vec<_> = parsed
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str() == Some("with-submodules"))
        .collect();

    for entry in with_submodules_entries {
        assert_eq!(entry["gitSubmodules"].as_bool(), Some(true));
    }

    let without_submodules_entries: Vec<_> = parsed
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str() == Some("without-submodules"))
        .collect();

    for entry in without_submodules_entries {
        assert!(entry.get("gitSubmodules").is_none() || entry["gitSubmodules"].is_null());
    }
}
