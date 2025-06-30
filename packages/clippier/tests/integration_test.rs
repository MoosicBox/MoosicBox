use clippier::{
    CargoLock, CargoLockPackage, build_external_dependency_map,
    find_packages_affected_by_external_deps, find_transitively_affected_external_deps,
};
use tempfile::TempDir;

/// Create a test workspace structure with Cargo.toml files
fn create_test_workspace() -> (TempDir, Vec<String>) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Create workspace root Cargo.toml
    let workspace_cargo_toml = temp_dir.path().join("Cargo.toml");
    std::fs::write(
        &workspace_cargo_toml,
        r#"
[workspace]
members = [
    "packages/api",
    "packages/client", 
    "packages/models"
]

[workspace.dependencies]
serde = "1.0"
tokio = "1.0"
reqwest = "0.11"
"#,
    )
    .expect("Failed to write workspace Cargo.toml");

    // Create package directories
    std::fs::create_dir_all(temp_dir.path().join("packages/api/src")).unwrap();
    std::fs::create_dir_all(temp_dir.path().join("packages/client/src")).unwrap();
    std::fs::create_dir_all(temp_dir.path().join("packages/models/src")).unwrap();

    // Create API package Cargo.toml (depends on models, reqwest, serde)
    std::fs::write(
        temp_dir.path().join("packages/api/Cargo.toml"),
        r#"
[package]
name = "api"
version = "0.1.0"
edition = "2021"

[dependencies]
models = { path = "../models" }
reqwest = { workspace = true }
serde = { workspace = true }
tokio = { workspace = true }
"#,
    )
    .unwrap();

    // Create Client package Cargo.toml (depends on models, reqwest)
    std::fs::write(
        temp_dir.path().join("packages/client/Cargo.toml"),
        r#"
[package]
name = "client" 
version = "0.1.0"
edition = "2021"

[dependencies]
models = { path = "../models" }
reqwest = { workspace = true }
"#,
    )
    .unwrap();

    // Create Models package Cargo.toml (depends on serde)
    std::fs::write(
        temp_dir.path().join("packages/models/Cargo.toml"),
        r#"
[package]
name = "models"
version = "0.1.0" 
edition = "2021"

[dependencies]
serde = { workspace = true }
"#,
    )
    .unwrap();

    let workspace_members = vec![
        "packages/api".to_string(),
        "packages/client".to_string(),
        "packages/models".to_string(),
    ];

    (temp_dir, workspace_members)
}

#[test]
fn test_build_external_dependency_map() {
    let (temp_dir, workspace_members) = create_test_workspace();

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

#[test]
fn test_end_to_end_external_dependency_analysis() {
    let (temp_dir, workspace_members) = create_test_workspace();

    // Create a comprehensive Cargo.lock that matches our test workspace
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            // External dependencies
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.195".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "serde_derive".to_string(),
                version: "1.0.195".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "tokio".to_string(),
                version: "1.35.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["pin-project-lite 0.2.0".to_string()]),
            },
            CargoLockPackage {
                name: "pin-project-lite".to_string(),
                version: "0.2.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "reqwest".to_string(),
                version: "0.11.24".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec![
                    "serde 1.0.195".to_string(),
                    "serde_json 1.0.111".to_string(),
                    "tokio 1.35.0".to_string(),
                ]),
            },
            CargoLockPackage {
                name: "serde_json".to_string(),
                version: "1.0.111".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["serde 1.0.195".to_string(), "itoa 1.0.0".to_string()]),
            },
            CargoLockPackage {
                name: "itoa".to_string(),
                version: "1.0.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            // Workspace packages
            CargoLockPackage {
                name: "api".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec![
                    "models 0.1.0".to_string(),
                    "reqwest 0.11.24".to_string(),
                    "serde 1.0.195".to_string(),
                    "tokio 1.35.0".to_string(),
                ]),
            },
            CargoLockPackage {
                name: "client".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec![
                    "models 0.1.0".to_string(),
                    "reqwest 0.11.24".to_string(),
                ]),
            },
            CargoLockPackage {
                name: "models".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["serde 1.0.195".to_string()]),
            },
        ],
    };

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

#[test]
fn test_deep_transitive_dependency_change() {
    let (temp_dir, workspace_members) = create_test_workspace();

    // Create a scenario where a deep dependency changes that affects many levels
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            // Deep dependency that changes
            CargoLockPackage {
                name: "libc".to_string(),
                version: "0.2.150".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            // Level 1: depends on libc
            CargoLockPackage {
                name: "socket2".to_string(),
                version: "0.5.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["libc 0.2.150".to_string()]),
            },
            // Level 2: depends on socket2
            CargoLockPackage {
                name: "mio".to_string(),
                version: "0.8.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["socket2 0.5.0".to_string()]),
            },
            // Level 3: depends on mio
            CargoLockPackage {
                name: "tokio".to_string(),
                version: "1.35.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["mio 0.8.0".to_string()]),
            },
            // Level 4: depends on tokio
            CargoLockPackage {
                name: "reqwest".to_string(),
                version: "0.11.24".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["tokio 1.35.0".to_string()]),
            },
            // Other deps
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.195".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            // Workspace packages
            CargoLockPackage {
                name: "api".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec![
                    "reqwest 0.11.24".to_string(),
                    "tokio 1.35.0".to_string(),
                ]),
            },
            CargoLockPackage {
                name: "client".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["reqwest 0.11.24".to_string()]),
            },
            CargoLockPackage {
                name: "models".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["serde 1.0.195".to_string()]),
            },
        ],
    };

    // Test scenario: libc changes (deep dependency)
    let directly_changed_deps = vec!["libc".to_string()];
    let all_affected_external_deps =
        find_transitively_affected_external_deps(&cargo_lock, &directly_changed_deps);

    insta::assert_debug_snapshot!(all_affected_external_deps, @r###"
    [
        "api",
        "client",
        "libc",
        "mio",
        "reqwest",
        "socket2",
        "tokio",
    ]
    "###);

    // Build external dependency map
    let external_dep_map = build_external_dependency_map(temp_dir.path(), &workspace_members)
        .expect("Failed to build external dependency map");

    // Find workspace packages affected
    let affected_workspace_packages =
        find_packages_affected_by_external_deps(&external_dep_map, &all_affected_external_deps);

    // Should include both api and client because they depend on reqwest/tokio
    // but NOT models because it only depends on serde
    insta::assert_debug_snapshot!(affected_workspace_packages, @r###"
    [
        "api",
        "client",
    ]
    "###);
}

#[test]
fn test_no_transitive_impact() {
    let (temp_dir, workspace_members) = create_test_workspace();

    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.195".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "reqwest".to_string(),
                version: "0.11.24".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None, // No dependencies
            },
            CargoLockPackage {
                name: "tokio".to_string(),
                version: "1.35.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None, // No dependencies
            },
            // Independent external dependency that no workspace package uses
            CargoLockPackage {
                name: "some-unused-crate".to_string(),
                version: "1.0.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            // Workspace packages
            CargoLockPackage {
                name: "api".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["reqwest 0.11.24".to_string()]),
            },
            CargoLockPackage {
                name: "client".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["reqwest 0.11.24".to_string()]),
            },
            CargoLockPackage {
                name: "models".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["serde 1.0.195".to_string()]),
            },
        ],
    };

    // Test scenario: unused crate changes
    let directly_changed_deps = vec!["some-unused-crate".to_string()];
    let all_affected_external_deps =
        find_transitively_affected_external_deps(&cargo_lock, &directly_changed_deps);

    insta::assert_debug_snapshot!(all_affected_external_deps, @r###"
    [
        "some-unused-crate",
    ]
    "###);

    // Build external dependency map
    let external_dep_map = build_external_dependency_map(temp_dir.path(), &workspace_members)
        .expect("Failed to build external dependency map");

    // Find workspace packages affected
    let affected_workspace_packages =
        find_packages_affected_by_external_deps(&external_dep_map, &all_affected_external_deps);

    // Should be empty since no workspace package uses some-unused-crate
    insta::assert_debug_snapshot!(affected_workspace_packages, @r###"[]"###);
}
