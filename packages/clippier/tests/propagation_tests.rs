use clippier::{OutputType, handle_features_command};
use clippier_test_utilities::test_resources::load_test_workspace;

#[test]
fn test_git_submodules_propagates_through_build_deps() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["middle".to_string()]),
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    let middle = packages
        .iter()
        .find(|p| p["name"] == "middle")
        .expect("middle package not found");

    assert_eq!(
        middle["gitSubmodules"], true,
        "git-submodules should propagate from leaf to middle via build-dependencies"
    );
}

#[test]
fn test_git_submodules_propagates_through_dev_and_regular_deps() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["root".to_string()]),
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    let root = packages
        .iter()
        .find(|p| p["name"] == "root")
        .expect("root package not found");

    assert_eq!(
        root["gitSubmodules"], true,
        "git-submodules should propagate from leaf to root via dependencies"
    );
}

#[test]
fn test_dependencies_propagate_and_merge() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["middle".to_string()]),
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    let middle = packages
        .iter()
        .find(|p| p["name"] == "middle")
        .expect("middle package not found");

    let deps = middle["dependencies"]
        .as_str()
        .expect("dependencies should be a string");

    assert!(
        deps.contains("libfoo-dev"),
        "Should inherit libfoo from leaf"
    );
    assert!(
        deps.contains("libbar-dev"),
        "Should inherit libbar from leaf"
    );
    assert!(
        deps.contains("libmiddle-dev"),
        "Should have its own libmiddle"
    );
}

#[test]
fn test_dependencies_propagate_to_root() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["root".to_string()]),
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    let root = packages
        .iter()
        .find(|p| p["name"] == "root")
        .expect("root package not found");

    let deps = root["dependencies"]
        .as_str()
        .expect("dependencies should be a string");

    assert!(
        deps.contains("libfoo-dev"),
        "Should inherit libfoo from leaf"
    );
    assert!(
        deps.contains("libbar-dev"),
        "Should inherit libbar from leaf"
    );
    assert!(
        deps.contains("libmiddle-dev"),
        "Should inherit libmiddle from middle"
    );
}

#[test]
fn test_ci_steps_propagate_and_preserve_order() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["root".to_string()]),
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    let root = packages
        .iter()
        .find(|p| p["name"] == "root")
        .expect("root package not found");

    let ci_steps = root["ciSteps"]
        .as_str()
        .expect("ciSteps should be a string");

    assert!(
        ci_steps.contains("curl -o vectors.tar.gz"),
        "Should inherit leaf ci-steps"
    );
    assert!(
        ci_steps.contains("tar -xzf vectors.tar.gz"),
        "Should inherit leaf ci-steps"
    );
    assert!(
        ci_steps.contains("Setting up middle"),
        "Should inherit middle ci-steps"
    );
    assert!(
        ci_steps.contains("cargo test"),
        "Should have its own ci-steps"
    );
}

#[test]
fn test_env_vars_propagate_with_overlay() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["root".to_string()]),
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    let root = packages
        .iter()
        .find(|p| p["name"] == "root")
        .expect("root package not found");

    let env = root["env"].as_str().expect("env should be a string");

    assert!(
        env.contains("FOO_PATH=\"/override/foo\""),
        "Root should override FOO_PATH"
    );

    assert!(
        env.contains("BAR_VERSION=\"3.0\""),
        "Should have BAR_VERSION from middle (middle overrides leaf)"
    );

    assert!(
        env.contains("MIDDLE_VAR=\"middle_value\""),
        "Should inherit MIDDLE_VAR from middle"
    );

    assert!(
        env.contains("ROOT_VAR=\"root_value\""),
        "Should have ROOT_VAR from root config"
    );
}

#[test]
fn test_nightly_does_not_propagate() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["leaf".to_string()]),
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    let leaf = packages
        .iter()
        .find(|p| p["name"] == "leaf")
        .expect("leaf package not found");

    assert_eq!(leaf["nightly"], false, "leaf should not be nightly");
}

#[test]
fn test_propagation_with_all_workspace_packages() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
    );

    assert!(result.is_ok(), "Command failed: {:?}", result.err());
    let output = result.unwrap();

    let packages: Vec<serde_json::Value> =
        serde_json::from_str(&output).expect("Failed to parse JSON");

    assert_eq!(packages.len(), 3, "Should have 3 packages");

    for pkg in &packages {
        assert_eq!(
            pkg["gitSubmodules"], true,
            "{} should have gitSubmodules propagated",
            pkg["name"]
        );
    }
}

#[test]
fn test_external_deps_dont_break_propagation() {
    let (temp_dir, _) = load_test_workspace("propagation");
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
        Some(&["leaf".to_string()]),
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
    );

    assert!(
        result.is_ok(),
        "Command should succeed even with external deps like serde"
    );
}
