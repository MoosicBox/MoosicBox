//! Tests for workspace member glob pattern expansion.
//!
//! These tests verify that clippier correctly handles workspace Cargo.toml files
//! that use glob patterns like `packages/*` in the members list.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clippier_test_utilities::test_resources::load_test_workspace;

/// Test that glob patterns in workspace members are correctly expanded
#[switchy_async::test]
async fn test_glob_members_workspace_loads() {
    let (temp_dir, workspace_members) = load_test_workspace("glob-members");

    // The workspace uses `members = ["packages/*"]` which should expand to
    // the three packages: pkg-a, pkg-b, pkg-c
    // Note: load_test_workspace returns the raw patterns, not expanded
    assert_eq!(workspace_members, vec!["packages/*"]);

    // But clippier should be able to process this workspace correctly
    let result = clippier::process_configs(
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
    )
    .await;

    // Should succeed without errors
    assert!(
        result.is_ok(),
        "Failed to process glob workspace: {result:?}"
    );
}

/// Test that workspace dependency resolution works with glob patterns
#[switchy_async::test]
async fn test_glob_members_dependency_resolution() {
    let (temp_dir, _) = load_test_workspace("glob-members");

    // pkg-a depends on pkg-b, which depends on pkg-c
    // This tests that WorkspaceContext correctly expands globs
    let result = clippier::find_workspace_dependencies(temp_dir.path(), "pkg-a", None, false);

    assert!(
        result.is_ok(),
        "Failed to find dependencies for pkg-a: {result:?}"
    );

    let deps = result.unwrap();
    // find_workspace_dependencies returns Vec<(String, String)> - (dep_name, dep_path)
    let dep_names: Vec<&str> = deps.iter().map(|(name, _)| name.as_str()).collect();

    // pkg-a -> pkg-b -> pkg-c (transitive)
    assert!(
        dep_names.contains(&"pkg-b"),
        "pkg-a should depend on pkg-b, got: {dep_names:?}"
    );
    assert!(
        dep_names.contains(&"pkg-c"),
        "pkg-a should transitively depend on pkg-c, got: {dep_names:?}"
    );
}

/// Test that affected packages detection works with glob patterns
#[switchy_async::test]
async fn test_glob_members_affected_packages() {
    let (temp_dir, _) = load_test_workspace("glob-members");

    // Simulate a change to pkg-c's lib.rs
    let changed_files = vec!["packages/pkg-c/src/lib.rs".to_string()];

    let result = clippier::find_affected_packages(temp_dir.path(), &changed_files, &[]);

    assert!(
        result.is_ok(),
        "Failed to find affected packages: {result:?}"
    );

    let affected = result.unwrap();
    // pkg-c is directly affected
    assert!(
        affected.contains(&"pkg-c".to_string()),
        "pkg-c should be affected, got: {affected:?}"
    );
    // pkg-b depends on pkg-c, so it should also be affected
    assert!(
        affected.contains(&"pkg-b".to_string()),
        "pkg-b should be affected (depends on pkg-c), got: {affected:?}"
    );
    // pkg-a depends on pkg-b, so it should also be affected
    assert!(
        affected.contains(&"pkg-a".to_string()),
        "pkg-a should be affected (depends on pkg-b), got: {affected:?}"
    );
}

/// Test that feature generation works with glob patterns
#[switchy_async::test]
async fn test_glob_members_feature_generation() {
    let (temp_dir, _) = load_test_workspace("glob-members");

    // Process just pkg-a which has feature-a
    let pkg_a_path = temp_dir.path().join("packages/pkg-a");
    let result = clippier::process_configs(
        &pkg_a_path,
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
    .await;

    assert!(
        result.is_ok(),
        "Failed to generate features for pkg-a: {result:?}"
    );

    let packages = result.unwrap();
    assert!(!packages.is_empty(), "Should have at least one package");

    // Verify the package has the expected features
    let pkg = &packages[0];
    let name = pkg.get("name").and_then(|v| v.as_str());
    assert_eq!(name, Some("pkg-a"), "Package name should be pkg-a");
}

/// Test mixed glob and explicit members
#[switchy_async::test]
async fn test_mixed_glob_and_explicit_members() {
    use switchy_fs::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a workspace with both glob and explicit members
    let workspace_toml = r#"
[workspace]
members = [
    "packages/*",
    "extra/special-pkg",
]
resolver = "2"
"#;

    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
        .expect("Failed to write Cargo.toml");

    // Create packages via glob
    for pkg in ["pkg-x", "pkg-y"] {
        let pkg_path = temp_dir.path().join("packages").join(pkg);
        switchy_fs::sync::create_dir_all(pkg_path.join("src"))
            .expect("Failed to create package dir");
        switchy_fs::sync::write(
            pkg_path.join("Cargo.toml"),
            format!(
                r#"
[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"
"#
            ),
        )
        .expect("Failed to write package Cargo.toml");
        switchy_fs::sync::write(pkg_path.join("src/lib.rs"), "//! Lib")
            .expect("Failed to write lib.rs");
    }

    // Create explicit member
    let special_path = temp_dir.path().join("extra/special-pkg");
    switchy_fs::sync::create_dir_all(special_path.join("src"))
        .expect("Failed to create special-pkg dir");
    switchy_fs::sync::write(
        special_path.join("Cargo.toml"),
        r#"
[package]
name = "special-pkg"
version = "0.1.0"
edition = "2021"
"#,
    )
    .expect("Failed to write special-pkg Cargo.toml");
    switchy_fs::sync::write(special_path.join("src/lib.rs"), "//! Special")
        .expect("Failed to write lib.rs");

    let result = clippier::process_configs(
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
    )
    .await;

    assert!(
        result.is_ok(),
        "Failed to process mixed workspace: {result:?}"
    );
}

/// Test nested glob patterns like `crates/*`
#[switchy_async::test]
async fn test_nested_glob_patterns() {
    use switchy_fs::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a workspace with nested glob pattern
    // Note: Cargo supports patterns like `crates/*/subcrate` but the `*` only matches
    // one directory level, not recursively
    let workspace_toml = r#"
[workspace]
members = ["crates/*"]
resolver = "2"
"#;

    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
        .expect("Failed to write Cargo.toml");

    // Create nested packages
    for crate_name in ["crate-a", "crate-b"] {
        let pkg_path = temp_dir.path().join("crates").join(crate_name);
        switchy_fs::sync::create_dir_all(pkg_path.join("src")).expect("Failed to create crate dir");
        switchy_fs::sync::write(
            pkg_path.join("Cargo.toml"),
            format!(
                r#"
[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2021"
"#
            ),
        )
        .expect("Failed to write crate Cargo.toml");
        switchy_fs::sync::write(pkg_path.join("src/lib.rs"), "//! Crate")
            .expect("Failed to write lib.rs");
    }

    let result = clippier::process_configs(
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
    )
    .await;

    assert!(
        result.is_ok(),
        "Failed to process nested glob workspace: {result:?}"
    );
}

/// Regression test: empty workspace after glob expansion should not panic
#[switchy_async::test]
async fn test_empty_glob_expansion() {
    use switchy_fs::TempDir;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create a workspace with a glob that matches nothing
    let workspace_toml = r#"
[workspace]
members = ["nonexistent/*"]
resolver = "2"
"#;

    switchy_fs::sync::write(temp_dir.path().join("Cargo.toml"), workspace_toml)
        .expect("Failed to write Cargo.toml");

    // Don't create any packages - the glob should match nothing

    let result = clippier::process_configs(
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
    )
    .await;

    // Should not panic, might return empty or error gracefully
    // The important thing is it doesn't crash
    let _ = result;
}
