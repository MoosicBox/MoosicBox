use clippier::{
    CargoLock, CargoLockPackage, find_transitively_affected_external_deps, parse_cargo_lock_changes,
};

#[test]
fn test_real_world_tokio_update() {
    // Simulate a real git diff where tokio was updated from 1.35.0 to 1.36.0
    let cargo_lock_changes = vec![
        (' ', "[[package]]".to_string()),
        (' ', "name = \"tokio\"".to_string()),
        ('-', "version = \"1.35.0\"".to_string()),
        ('+', "version = \"1.36.0\"".to_string()),
        (
            ' ',
            "source = \"registry+https://github.com/rust-lang/crates.io-index\"".to_string(),
        ),
    ];

    let changed_deps = parse_cargo_lock_changes(&cargo_lock_changes);
    insta::assert_debug_snapshot!(changed_deps, @r###"
    [
        "tokio",
    ]
    "###);

    // Create a Cargo.lock representing the dependency graph
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            CargoLockPackage {
                name: "tokio".to_string(),
                version: "1.36.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "hyper".to_string(),
                version: "0.14.28".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["tokio 1.36.0".to_string()]),
            },
            CargoLockPackage {
                name: "reqwest".to_string(),
                version: "0.11.24".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["hyper 0.14.28".to_string()]),
            },
            // Workspace packages
            CargoLockPackage {
                name: "moosicbox_server".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["reqwest 0.11.24".to_string()]),
            },
            CargoLockPackage {
                name: "moosicbox_models".to_string(),
                version: "0.1.0".to_string(),
                source: None,
                dependencies: Some(vec!["serde 1.0.195".to_string()]),
            },
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.195".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
        ],
    };

    // Find all packages affected by the tokio update
    let all_affected = find_transitively_affected_external_deps(&cargo_lock, &changed_deps);

    // Should include tokio, hyper, reqwest, and moosicbox_server
    // but NOT moosicbox_models (only depends on serde)
    insta::assert_debug_snapshot!(all_affected, @r###"
    [
        "hyper",
        "moosicbox_server",
        "reqwest",
        "tokio",
    ]
    "###);
}
