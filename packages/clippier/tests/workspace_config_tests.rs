use std::path::PathBuf;

#[test]
fn test_workspace_config_defaults() {
    // Test pkg1 which has no package-level config - should use workspace defaults
    let workspace_path =
        PathBuf::from("test-resources/workspaces/workspace-config-test/packages/pkg1");
    let workspace_root = PathBuf::from("test-resources/workspaces/workspace-config-test");

    let workspace_context = clippier::WorkspaceContext::new(&workspace_root).unwrap();
    let result = clippier::process_configs(
        &workspace_path,
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
    .expect("Failed to process configs");

    assert_eq!(result.len(), 1);

    let config = &result[0];

    // Check nightly from workspace
    assert_eq!(config.get("nightly").and_then(|v| v.as_bool()), Some(false));

    // Check env vars from workspace
    let env = config.get("env").and_then(|v| v.as_str()).unwrap_or("");
    assert!(env.contains("WORKSPACE_VAR=\"from_workspace\""));
    assert!(env.contains("RUST_BACKTRACE=\"1\""));

    // Check ci steps from workspace
    let ci_steps = config.get("ciSteps").and_then(|v| v.as_str()).unwrap_or("");
    assert!(ci_steps.contains("echo workspace ci step"));

    // Check dependencies from workspace
    let deps = config
        .get("dependencies")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(deps.contains("apt-get install workspace-dep"));
}

#[test]
fn test_workspace_config_with_package_overrides() {
    // Test pkg2 which has package-level config - should merge with workspace defaults
    let workspace_path =
        PathBuf::from("test-resources/workspaces/workspace-config-test/packages/pkg2");
    let workspace_root = PathBuf::from("test-resources/workspaces/workspace-config-test");

    let workspace_context = clippier::WorkspaceContext::new(&workspace_root).unwrap();
    let result = clippier::process_configs(
        &workspace_path,
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
    .expect("Failed to process configs");

    assert_eq!(result.len(), 1);

    let config = &result[0];

    // Check nightly from workspace (not overridden)
    assert_eq!(config.get("nightly").and_then(|v| v.as_bool()), Some(false));

    // Check env vars - should have both workspace and package vars
    let env = config.get("env").and_then(|v| v.as_str()).unwrap_or("");
    assert!(env.contains("WORKSPACE_VAR=\"from_workspace\""));
    assert!(env.contains("RUST_BACKTRACE=\"1\""));
    assert!(env.contains("PACKAGE_VAR=\"from_package\""));

    // Check ci steps from workspace
    let ci_steps = config.get("ciSteps").and_then(|v| v.as_str()).unwrap_or("");
    assert!(ci_steps.contains("echo workspace ci step"));

    // Check dependencies - should have both
    let deps = config
        .get("dependencies")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(deps.contains("apt-get install workspace-dep"));
    assert!(deps.contains("apt-get install package-dep"));
}

#[test]
fn test_backward_compatibility_no_workspace_config() {
    // Test that workspaces without workspace-level config still work
    let workspace_path = PathBuf::from("test-resources/workspaces/propagation/root");
    let workspace_root = PathBuf::from("test-resources/workspaces/propagation");

    let workspace_context = clippier::WorkspaceContext::new(&workspace_root).unwrap();
    let result = clippier::process_configs(
        &workspace_path,
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
    .expect("Failed to process configs");

    assert_eq!(result.len(), 1);

    let config = &result[0];

    // Should still work as before
    assert_eq!(config.get("name").and_then(|v| v.as_str()), Some("root"));
}
