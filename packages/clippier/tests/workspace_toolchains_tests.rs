//! Tests for the workspace-toolchains command functionality

use clippier::{OutputType, handle_workspace_toolchains_command};
use clippier_test_utilities::test_resources::load_test_workspace;

#[switchy_async::test]
async fn test_workspace_toolchains_aggregates_all_packages() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    // Should have dependencies from workspace and packages
    let deps = parsed["dependencies"]
        .as_array()
        .expect("dependencies should be array");
    assert!(!deps.is_empty(), "Should have dependencies");

    // Should have toolchains from workspace clippier.toml
    let toolchains = parsed["toolchains"]
        .as_array()
        .expect("toolchains should be array");
    let toolchain_strs: Vec<&str> = toolchains.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(
        toolchain_strs.contains(&"cargo-machete"),
        "Should contain cargo-machete toolchain"
    );
    assert!(
        toolchain_strs.contains(&"taplo"),
        "Should contain taplo toolchain"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_tracks_nightly_packages() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    let nightly_packages = parsed["nightly_packages"]
        .as_array()
        .expect("nightly_packages should be array");
    let nightly_strs: Vec<&str> = nightly_packages
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // pkg-nightly has nightly = true globally
    assert!(
        nightly_strs.contains(&"pkg-nightly"),
        "Should contain pkg-nightly"
    );

    // pkg-os-nightly has nightly = true only for ubuntu
    assert!(
        nightly_strs.contains(&"pkg-os-nightly"),
        "Should contain pkg-os-nightly (ubuntu has nightly=true)"
    );

    // pkg-stable should NOT be in nightly_packages
    assert!(
        !nightly_strs.contains(&"pkg-stable"),
        "Should NOT contain pkg-stable"
    );

    // pkg-no-config should NOT be in nightly_packages
    assert!(
        !nightly_strs.contains(&"pkg-no-config"),
        "Should NOT contain pkg-no-config"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_git_submodules() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    // git_submodules should be true because pkg-submodules has git-submodules = true
    assert_eq!(
        parsed["git_submodules"].as_bool(),
        Some(true),
        "git_submodules should be true"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_os_filtering() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    // Test ubuntu
    let ubuntu_result =
        handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
            .expect("Failed to run workspace-toolchains command for ubuntu");
    let ubuntu_parsed: serde_json::Value =
        serde_json::from_str(&ubuntu_result).expect("Failed to parse JSON output");

    let ubuntu_deps = ubuntu_parsed["dependencies"]
        .as_array()
        .expect("dependencies should be array");
    let ubuntu_deps_str: String = ubuntu_deps
        .iter()
        .map(|v| v.as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        ubuntu_deps_str.contains("apt-get"),
        "Ubuntu deps should contain apt-get commands"
    );

    // Test windows
    let windows_result =
        handle_workspace_toolchains_command(temp_dir.path(), "windows", OutputType::Json)
            .expect("Failed to run workspace-toolchains command for windows");
    let windows_parsed: serde_json::Value =
        serde_json::from_str(&windows_result).expect("Failed to parse JSON output");

    let windows_deps = windows_parsed["dependencies"]
        .as_array()
        .expect("dependencies should be array");
    let windows_deps_str: String = windows_deps
        .iter()
        .map(|v| v.as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        windows_deps_str.contains("vcpkg"),
        "Windows deps should contain vcpkg commands"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_env_vars() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    let env = parsed["env"].as_object().expect("env should be object");

    // From workspace clippier.toml
    assert_eq!(
        env.get("WORKSPACE_VAR").and_then(|v| v.as_str()),
        Some("from_workspace"),
        "Should have WORKSPACE_VAR from workspace config"
    );

    // From pkg-nightly clippier.toml
    assert_eq!(
        env.get("NIGHTLY_VAR").and_then(|v| v.as_str()),
        Some("from_nightly_pkg"),
        "Should have NIGHTLY_VAR from pkg-nightly"
    );

    // From pkg-stable clippier.toml
    assert_eq!(
        env.get("STABLE_VAR").and_then(|v| v.as_str()),
        Some("from_stable_pkg"),
        "Should have STABLE_VAR from pkg-stable"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_ci_steps() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    let ci_steps = parsed["ci_steps"]
        .as_array()
        .expect("ci_steps should be array");
    let ci_steps_str: String = ci_steps
        .iter()
        .map(|v| v.as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    // From pkg-nightly
    assert!(
        ci_steps_str.contains("echo nightly package setup"),
        "Should have CI step from pkg-nightly"
    );

    // From pkg-submodules
    assert!(
        ci_steps_str.contains("git submodule update"),
        "Should have CI step from pkg-submodules"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_no_workspace_config() {
    // Use the 'basic' workspace which has no root clippier.toml
    let (temp_dir, _) = load_test_workspace("basic");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    // Should still work, just with empty/default values
    assert!(parsed["dependencies"].is_array());
    assert!(parsed["toolchains"].is_array());
    assert!(parsed["nightly_packages"].is_array());
}

#[switchy_async::test]
async fn test_workspace_toolchains_extracts_package_names() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    let nightly_packages = parsed["nightly_packages"]
        .as_array()
        .expect("nightly_packages should be array");

    // Should use actual package names from Cargo.toml, not directory names
    for pkg in nightly_packages {
        let name = pkg.as_str().expect("package name should be string");
        // Package names should be the ones from Cargo.toml (with hyphens, not underscores from dirs)
        assert!(
            name.starts_with("pkg-"),
            "Package name should start with 'pkg-', got: {name}"
        );
    }
}

#[switchy_async::test]
async fn test_workspace_toolchains_os_specific_nightly() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    // On ubuntu, pkg-os-nightly should be in nightly_packages
    let ubuntu_result =
        handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
            .expect("Failed to run workspace-toolchains command");
    let ubuntu_parsed: serde_json::Value =
        serde_json::from_str(&ubuntu_result).expect("Failed to parse JSON output");

    let ubuntu_nightly: Vec<&str> = ubuntu_parsed["nightly_packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    assert!(
        ubuntu_nightly.contains(&"pkg-os-nightly"),
        "pkg-os-nightly should be in nightly_packages for ubuntu"
    );

    // On windows, pkg-os-nightly should NOT be in nightly_packages (nightly = false)
    let windows_result =
        handle_workspace_toolchains_command(temp_dir.path(), "windows", OutputType::Json)
            .expect("Failed to run workspace-toolchains command");
    let windows_parsed: serde_json::Value =
        serde_json::from_str(&windows_result).expect("Failed to parse JSON output");

    let windows_nightly: Vec<&str> = windows_parsed["nightly_packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    assert!(
        !windows_nightly.contains(&"pkg-os-nightly"),
        "pkg-os-nightly should NOT be in nightly_packages for windows"
    );

    // pkg-nightly should still be there (global nightly = true)
    assert!(
        windows_nightly.contains(&"pkg-nightly"),
        "pkg-nightly should be in nightly_packages for windows (global nightly)"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_raw_output() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Raw)
        .expect("Failed to run workspace-toolchains command");

    // Raw output should be human-readable
    assert!(
        result.contains("Dependencies:"),
        "Should have Dependencies section"
    );
    assert!(
        result.contains("Toolchains:"),
        "Should have Toolchains section"
    );
    assert!(result.contains("CI Steps:"), "Should have CI Steps section");
    assert!(
        result.contains("Nightly Packages:"),
        "Should have Nightly Packages section"
    );
    assert!(
        result.contains("Git Submodules:"),
        "Should have Git Submodules line"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_json_output_structure() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    // Verify all expected fields exist
    assert!(
        parsed["dependencies"].is_array(),
        "dependencies should be array"
    );
    assert!(
        parsed["toolchains"].is_array(),
        "toolchains should be array"
    );
    assert!(parsed["ci_steps"].is_array(), "ci_steps should be array");
    assert!(parsed["env"].is_object(), "env should be object");
    assert!(
        parsed["nightly_packages"].is_array(),
        "nightly_packages should be array"
    );
    assert!(
        parsed["git_submodules"].is_boolean(),
        "git_submodules should be boolean"
    );
}

#[switchy_async::test]
async fn test_workspace_toolchains_snapshot_ubuntu() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "ubuntu", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    insta::assert_json_snapshot!("workspace_toolchains_ubuntu", parsed);
}

#[switchy_async::test]
async fn test_workspace_toolchains_snapshot_windows() {
    let (temp_dir, _) = load_test_workspace("workspace-toolchains-test");

    let result = handle_workspace_toolchains_command(temp_dir.path(), "windows", OutputType::Json)
        .expect("Failed to run workspace-toolchains command");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("Failed to parse JSON output");

    insta::assert_json_snapshot!("workspace_toolchains_windows", parsed);
}
