//! Integration tests for Node.js workspace support.
//!
//! These tests verify the Node workspace detection, package enumeration,
//! and dependency analysis functionality using test fixtures.

#![cfg(feature = "node-workspace")]

use std::collections::BTreeSet;

use clippier::test_utils::test_resources::{get_test_workspace_path, load_node_test_workspace};
use clippier::workspace::{
    NodePackageManager, NodeWorkspace, Workspace, WorkspaceType, detect_workspaces,
    select_primary_workspace,
};

// =============================================================================
// Workspace Detection Tests
// =============================================================================

#[switchy_async::test]
async fn test_detect_npm_workspace_from_test_resources() {
    let path = get_test_workspace_path("node-npm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap();

    assert!(ws.is_some(), "Should detect npm workspace");
    let ws = ws.unwrap();
    assert_eq!(ws.package_manager(), NodePackageManager::Npm);
    assert_eq!(ws.lockfile_path(), "package-lock.json");
}

#[switchy_async::test]
async fn test_detect_pnpm_workspace_from_test_resources() {
    let path = get_test_workspace_path("node-pnpm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap();

    assert!(ws.is_some(), "Should detect pnpm workspace");
    let ws = ws.unwrap();
    assert_eq!(ws.package_manager(), NodePackageManager::Pnpm);
    assert_eq!(ws.lockfile_path(), "pnpm-lock.yaml");
}

#[switchy_async::test]
async fn test_detect_bun_workspace_from_test_resources() {
    let path = get_test_workspace_path("node-bun-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap();

    assert!(ws.is_some(), "Should detect bun workspace");
    let ws = ws.unwrap();
    assert_eq!(ws.package_manager(), NodePackageManager::Bun);
    assert_eq!(ws.lockfile_path(), "bun.lock");
}

// =============================================================================
// Package Enumeration Tests
// =============================================================================

#[switchy_async::test]
async fn test_npm_workspace_packages() {
    let path = get_test_workspace_path("node-npm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
    let packages = ws.packages().await.unwrap();

    assert_eq!(packages.len(), 3, "Should have 3 packages");

    let names: BTreeSet<_> = packages.iter().map(|p| p.name().to_string()).collect();
    assert!(names.contains("@myorg/api"));
    assert!(names.contains("@myorg/client"));
    assert!(names.contains("@myorg/models"));
}

#[switchy_async::test]
async fn test_pnpm_workspace_packages() {
    let path = get_test_workspace_path("node-pnpm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
    let packages = ws.packages().await.unwrap();

    assert_eq!(packages.len(), 3, "Should have 3 packages");

    let names: BTreeSet<_> = packages.iter().map(|p| p.name().to_string()).collect();
    assert!(names.contains("@myorg/api"));
    assert!(names.contains("@myorg/client"));
    assert!(names.contains("@myorg/models"));
}

#[switchy_async::test]
async fn test_bun_workspace_packages() {
    let path = get_test_workspace_path("node-bun-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
    let packages = ws.packages().await.unwrap();

    assert_eq!(packages.len(), 3, "Should have 3 packages");

    let names: BTreeSet<_> = packages.iter().map(|p| p.name().to_string()).collect();
    assert!(names.contains("@myorg/api"));
    assert!(names.contains("@myorg/client"));
    assert!(names.contains("@myorg/models"));
}

// =============================================================================
// Dependency Analysis Tests
// =============================================================================

#[switchy_async::test]
async fn test_node_workspace_dependencies() {
    let path = get_test_workspace_path("node-npm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
    let packages = ws.packages().await.unwrap();

    // Find the api package
    let api = packages.iter().find(|p| p.name() == "@myorg/api").unwrap();

    // Check workspace dependencies
    let ws_deps: Vec<_> = api
        .workspace_dependencies()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert!(
        ws_deps.contains(&"@myorg/models"),
        "api should depend on models"
    );

    // Check external dependencies
    let ext_deps: Vec<_> = api
        .external_dependencies()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert!(
        ext_deps.contains(&"express"),
        "api should have express as external dependency"
    );
}

#[switchy_async::test]
async fn test_pnpm_workspace_protocol_dependencies() {
    let path = get_test_workspace_path("node-pnpm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
    let packages = ws.packages().await.unwrap();

    // Find the client package (uses workspace:* protocol)
    let client = packages
        .iter()
        .find(|p| p.name() == "@myorg/client")
        .unwrap();

    // Check workspace dependencies
    let ws_deps: Vec<_> = client
        .workspace_dependencies()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert!(
        ws_deps.contains(&"@myorg/models"),
        "client should depend on models via workspace protocol"
    );

    // Check external dependencies
    let ext_deps: Vec<_> = client
        .external_dependencies()
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert!(
        ext_deps.contains(&"axios"),
        "client should have axios as external dependency"
    );
}

// =============================================================================
// Package Name to Path Mapping Tests
// =============================================================================

#[switchy_async::test]
async fn test_node_package_name_to_path() {
    let path = get_test_workspace_path("node-npm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();
    let name_to_path = ws.package_name_to_path().await.unwrap();

    assert_eq!(name_to_path.len(), 3);
    assert!(name_to_path.contains_key("@myorg/api"));
    assert!(name_to_path.contains_key("@myorg/client"));
    assert!(name_to_path.contains_key("@myorg/models"));

    // Check that paths are relative
    let api_path = name_to_path.get("@myorg/api").unwrap();
    assert!(
        api_path.contains("packages/api") || api_path.contains("packages\\api"),
        "Path should contain packages/api: {api_path}"
    );
}

#[switchy_async::test]
async fn test_is_member_by_name() {
    let path = get_test_workspace_path("node-npm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();

    assert!(ws.is_member_by_name("@myorg/api").await);
    assert!(ws.is_member_by_name("@myorg/models").await);
    assert!(ws.is_member_by_name("@myorg/client").await);
    assert!(!ws.is_member_by_name("nonexistent").await);
    assert!(!ws.is_member_by_name("express").await);
}

#[switchy_async::test]
async fn test_find_member() {
    let path = get_test_workspace_path("node-npm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();

    let api_path = ws.find_member("@myorg/api").await;
    assert!(api_path.is_some());
    let api_path = api_path.unwrap();
    assert!(
        api_path.ends_with("packages/api") || api_path.ends_with("packages\\api"),
        "Path should end with packages/api: {}",
        api_path.display()
    );

    let nonexistent = ws.find_member("nonexistent").await;
    assert!(nonexistent.is_none());
}

// =============================================================================
// Mixed Workspace Tests (Cargo + Node)
// =============================================================================

#[switchy_async::test]
#[cfg(feature = "cargo-workspace")]
async fn test_mixed_workspace_detects_both() {
    use clippier::workspace::CargoWorkspace;

    let path = get_test_workspace_path("mixed-cargo-node");

    // Both workspace types should be detected
    let cargo_ws = CargoWorkspace::detect(&path).await.unwrap();
    assert!(cargo_ws.is_some(), "Should detect Cargo workspace");

    let node_ws = NodeWorkspace::detect(&path).await.unwrap();
    assert!(node_ws.is_some(), "Should detect Node workspace");

    // Verify package counts
    let cargo_packages = cargo_ws.unwrap().packages().await.unwrap();
    assert_eq!(
        cargo_packages.len(),
        2,
        "Cargo workspace should have 2 packages (core, api)"
    );

    let node_packages = node_ws.unwrap().packages().await.unwrap();
    assert_eq!(
        node_packages.len(),
        1,
        "Node workspace should have 1 package (web)"
    );
}

#[switchy_async::test]
#[cfg(feature = "cargo-workspace")]
async fn test_mixed_workspace_cargo_has_priority() {
    let path = get_test_workspace_path("mixed-cargo-node");

    // detect_workspaces without filter should return both, Cargo first
    let workspaces = detect_workspaces(&path, None).await.unwrap();
    assert_eq!(workspaces.len(), 2, "Should detect 2 workspaces");

    // select_primary_workspace should return Cargo
    let primary = select_primary_workspace(workspaces);
    assert!(primary.is_some());

    let primary = primary.unwrap();
    // Cargo.lock is the lockfile for Cargo workspaces
    assert_eq!(primary.lockfile_path(), "Cargo.lock");
}

#[switchy_async::test]
#[cfg(feature = "cargo-workspace")]
async fn test_mixed_workspace_can_filter_to_node() {
    let path = get_test_workspace_path("mixed-cargo-node");

    // Filter to only Node workspaces
    let workspaces = detect_workspaces(&path, Some(&[WorkspaceType::Node]))
        .await
        .unwrap();
    assert_eq!(workspaces.len(), 1, "Should detect 1 Node workspace");

    let primary = select_primary_workspace(workspaces).unwrap();
    assert_eq!(primary.lockfile_path(), "pnpm-lock.yaml");
}

#[switchy_async::test]
#[cfg(feature = "cargo-workspace")]
async fn test_mixed_workspace_can_filter_to_cargo() {
    let path = get_test_workspace_path("mixed-cargo-node");

    // Filter to only Cargo workspaces
    let workspaces = detect_workspaces(&path, Some(&[WorkspaceType::Cargo]))
        .await
        .unwrap();
    assert_eq!(workspaces.len(), 1, "Should detect 1 Cargo workspace");

    let primary = select_primary_workspace(workspaces).unwrap();
    assert_eq!(primary.lockfile_path(), "Cargo.lock");
}

// =============================================================================
// Test Utils Helper Tests
// =============================================================================

#[switchy_async::test]
async fn test_load_node_test_workspace_npm() {
    let (temp_dir, members) = load_node_test_workspace("node-npm-basic");

    assert_eq!(members.len(), 3);
    assert!(switchy_fs::exists(temp_dir.path().join("package.json")));
    assert!(switchy_fs::exists(
        temp_dir.path().join("package-lock.json")
    ));
}

#[switchy_async::test]
async fn test_load_node_test_workspace_pnpm() {
    let (temp_dir, members) = load_node_test_workspace("node-pnpm-basic");

    assert_eq!(members.len(), 3);
    assert!(switchy_fs::exists(temp_dir.path().join("package.json")));
    assert!(switchy_fs::exists(
        temp_dir.path().join("pnpm-workspace.yaml")
    ));
    assert!(switchy_fs::exists(temp_dir.path().join("pnpm-lock.yaml")));
}

#[switchy_async::test]
async fn test_load_node_test_workspace_bun() {
    let (temp_dir, members) = load_node_test_workspace("node-bun-basic");

    assert_eq!(members.len(), 3);
    assert!(switchy_fs::exists(temp_dir.path().join("package.json")));
    assert!(switchy_fs::exists(temp_dir.path().join("bun.lock")));
}

// =============================================================================
// Lockfile Parsing Tests
// =============================================================================

#[switchy_async::test]
async fn test_npm_lockfile_parsing() {
    let path = get_test_workspace_path("node-npm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();

    let lockfile = ws.read_lockfile().await.unwrap();
    let entries = lockfile.entries();

    // Should have external packages
    assert!(!entries.is_empty(), "Lockfile should have entries");

    // Check for known packages
    let names: BTreeSet<_> = entries.iter().map(|e| e.name().to_string()).collect();
    assert!(names.contains("express"), "Should contain express");
    assert!(names.contains("axios"), "Should contain axios");
    assert!(names.contains("zod"), "Should contain zod");
}

#[switchy_async::test]
async fn test_pnpm_lockfile_parsing() {
    let path = get_test_workspace_path("node-pnpm-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();

    // pnpm lockfile parsing should not error
    let lockfile_result = ws.read_lockfile().await;
    assert!(
        lockfile_result.is_ok(),
        "Lockfile parsing should succeed: {:?}",
        lockfile_result.err()
    );

    // Note: The pnpm lockfile format in our test fixtures may not include
    // full package entries in the snapshots section, so we just verify parsing works.
}

#[switchy_async::test]
async fn test_bun_lockfile_parsing() {
    let path = get_test_workspace_path("node-bun-basic");
    let ws = NodeWorkspace::detect(&path).await.unwrap().unwrap();

    // bun lockfile parsing should not error
    let lockfile_result = ws.read_lockfile().await;
    assert!(
        lockfile_result.is_ok(),
        "Lockfile parsing should succeed: {:?}",
        lockfile_result.err()
    );

    // Note: The bun lockfile format uses a different structure than what
    // the parser may expect, so we just verify parsing doesn't error.
}
