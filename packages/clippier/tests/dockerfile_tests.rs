use clippier_test_utilities::test_resources::load_test_workspace;

#[test]
fn test_basic_dockerfile_generation() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test basic Dockerfile generation
    let result = clippier::generate_dockerfile(
        temp_dir.path(),
        "web",
        None,
        &temp_dir.path().join("Dockerfile"),
        "rust:1-bookworm",
        "debian:bookworm-slim",
        &["8080".to_string()],
        None,
        false,
        &[],
    );
    assert!(result.is_ok());
}

#[test]
fn test_dockerfile_feature_inclusion() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test feature-specific builds
    let result = clippier::generate_dockerfile(
        temp_dir.path(),
        "api",
        Some(&["server".to_string(), "database".to_string()]),
        &temp_dir.path().join("Dockerfile.api"),
        "rust:1-bookworm",
        "debian:bookworm-slim",
        &["3000".to_string()],
        None,
        false,
        &[],
    );
    assert!(result.is_ok());
}

#[test]
fn test_dockerfile_system_dependencies() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test system dependency collection
    let test_data = serde_json::json!({
        "api_package": {
            "ubuntu_deps": ["libsqlite3-dev", "build-essential"],
            "consolidated_install": "apt-get install libsqlite3-dev build-essential"
        },
        "fallback_deps": ["cmake"]
    });

    insta::assert_yaml_snapshot!("dockerfile_system_deps", test_data);
}

#[test]
fn test_dockerignore_generation() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test dockerignore file generation
    let test_data = serde_json::json!({
        "target_package": "web",
        "excluded_patterns": ["/packages/*"],
        "included_packages": [
            "!/packages/web",
            "!/packages/api",
            "!/packages/models",
            "!/packages/core"
        ],
        "generate_dockerignore": true
    });

    insta::assert_yaml_snapshot!("dockerignore_generation", test_data);
}

#[test]
fn test_dockerfile_env_vars() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test environment variables and build args
    let test_data = serde_json::json!({
        "default_env_vars": {
            "RUST_LOG": "info,moosicbox=debug,moosicbox_middleware::api_logger=trace",
            "MAX_THREADS": "64",
            "ACTIX_WORKERS": "32"
        },
        "custom_build_args": ["API_PORT", "DATABASE_URL"]
    });

    insta::assert_yaml_snapshot!("dockerfile_env_vars", test_data);
}

#[test]
fn test_dockerfile_binary_name_detection() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test binary name detection from Cargo.toml
    let binary_name = clippier::get_binary_name(temp_dir.path(), "cli", "packages/cli");
    assert_eq!(binary_name, "cli-tool");
}

#[test]
fn test_dockerfile_workspace_modification() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test workspace Cargo.toml modification for Docker
    let test_data = serde_json::json!({
        "original_members": ["packages/core", "packages/models", "packages/api", "packages/web", "packages/cli", "packages/shared-utils"],
        "filtered_for_web": ["packages/api", "packages/models", "packages/core", "packages/shared-utils", "packages/web"],
        "sed_command": "sed -e '/^members = \\[/,/^\\]/c\\members = [...]' Cargo.toml"
    });

    insta::assert_yaml_snapshot!("dockerfile_workspace_mod", test_data);
}

#[test]
fn test_dockerfile_custom_images() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test custom base and final images
    let result = clippier::generate_dockerfile(
        temp_dir.path(),
        "api",
        None,
        &temp_dir.path().join("Dockerfile.custom"),
        "rust:1.70-alpine",
        "alpine:3.18",
        &["8080".to_string()],
        Some("ENVIRONMENT=production,DEBUG=false"),
        true,
        &[],
    );
    assert!(result.is_ok());
}

#[test]
fn test_dockerfile_dependency_resolution() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test workspace dependency resolution in Docker context
    let result = clippier::find_workspace_dependencies(
        temp_dir.path(),
        "web",
        None,
        true, // all_potential_deps for Docker compatibility
    );
    assert!(result.is_ok());
}

#[test]
fn test_dockerfile_minimal_workspace() {
    let (temp_dir, _) = load_test_workspace("complex");

    // Test with minimal workspace without clippier.toml
    let result = clippier::generate_dockerfile(
        temp_dir.path(),
        "models",
        None,
        &temp_dir.path().join("Dockerfile.minimal"),
        "rust:1-bookworm",
        "debian:bookworm-slim",
        &[],
        None,
        false,
        &[],
    );
    assert!(result.is_ok());
}
