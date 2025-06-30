use clippier::{
    CargoLock, CargoLockPackage, find_transitively_affected_external_deps, parse_cargo_lock,
    parse_cargo_lock_changes, parse_dependency_name,
};

#[test]
fn test_parse_dependency_name() {
    insta::assert_debug_snapshot!(parse_dependency_name("serde"), @r###""serde""###);
    insta::assert_debug_snapshot!(parse_dependency_name("serde 1.0.0"), @r###""serde""###);
    insta::assert_debug_snapshot!(
        parse_dependency_name("tokio 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"),
        @r###""tokio""###
    );
    insta::assert_debug_snapshot!(parse_dependency_name("serde_json 1.0.0"), @r###""serde_json""###);
}

#[test]
fn test_parse_cargo_lock_changes_simple() {
    let changes = vec![
        (' ', "".to_string()),
        (' ', "[[package]]".to_string()),
        (' ', "name = \"serde\"".to_string()),
        ('-', "version = \"1.0.0\"".to_string()),
        ('+', "version = \"1.0.1\"".to_string()),
        (' ', "".to_string()),
        (' ', "[[package]]".to_string()),
        (' ', "name = \"tokio\"".to_string()),
        (' ', "version = \"1.0.0\"".to_string()),
    ];

    let result = parse_cargo_lock_changes(&changes);
    insta::assert_debug_snapshot!(result, @r###"
    [
        "serde",
    ]
    "###);
}

#[test]
fn test_parse_cargo_lock_changes_multiple() {
    let changes = vec![
        (' ', "[[package]]".to_string()),
        (' ', "name = \"serde\"".to_string()),
        ('-', "version = \"1.0.0\"".to_string()),
        ('+', "version = \"1.0.1\"".to_string()),
        (' ', "".to_string()),
        (' ', "[[package]]".to_string()),
        (' ', "name = \"serde_json\"".to_string()),
        ('-', "version = \"1.0.0\"".to_string()),
        ('+', "version = \"1.0.1\"".to_string()),
        (' ', "".to_string()),
        (' ', "[[package]]".to_string()),
        (' ', "name = \"tokio\"".to_string()),
        (' ', "version = \"1.0.0\"".to_string()),
    ];

    let result = parse_cargo_lock_changes(&changes);
    insta::assert_debug_snapshot!(result, @r###"
    [
        "serde",
        "serde_json",
    ]
    "###);
}

#[test]
fn test_simple_transitive_dependencies() {
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "serde_json".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["serde 1.0.1".to_string()]),
            },
            CargoLockPackage {
                name: "reqwest".to_string(),
                version: "0.11.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec![
                    "serde_json 1.0.1".to_string(),
                    "tokio 1.0.0".to_string(),
                ]),
            },
            CargoLockPackage {
                name: "tokio".to_string(),
                version: "1.0.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
        ],
    };

    let directly_changed = vec!["serde".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    insta::assert_debug_snapshot!(result, @r###"
    [
        "reqwest",
        "serde",
        "serde_json",
    ]
    "###);
}

#[test]
fn test_complex_transitive_dependencies() {
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            // Base dependency that changes
            CargoLockPackage {
                name: "openssl-sys".to_string(),
                version: "0.9.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            // Direct dependent
            CargoLockPackage {
                name: "openssl".to_string(),
                version: "0.10.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["openssl-sys 0.9.0".to_string()]),
            },
            // Second level dependent
            CargoLockPackage {
                name: "native-tls".to_string(),
                version: "0.2.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["openssl 0.10.0".to_string()]),
            },
            // Third level dependent
            CargoLockPackage {
                name: "reqwest".to_string(),
                version: "0.11.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec![
                    "native-tls 0.2.0".to_string(),
                    "serde_json 1.0.1".to_string(),
                ]),
            },
            // Unrelated dependency that also uses reqwest
            CargoLockPackage {
                name: "my-http-client".to_string(),
                version: "0.1.0".to_string(),
                source: None, // workspace package
                dependencies: Some(vec!["reqwest 0.11.0".to_string()]),
            },
            // Independent dependency
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "serde_json".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["serde 1.0.1".to_string()]),
            },
        ],
    };

    let directly_changed = vec!["openssl-sys".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    insta::assert_debug_snapshot!(result, @r###"
    [
        "my-http-client",
        "native-tls",
        "openssl",
        "openssl-sys",
        "reqwest",
    ]
    "###);
}

#[test]
fn test_multiple_changed_dependencies() {
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "tokio".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "serde_json".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["serde 1.0.1".to_string()]),
            },
            CargoLockPackage {
                name: "reqwest".to_string(),
                version: "0.11.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec![
                    "serde_json 1.0.1".to_string(),
                    "tokio 1.0.1".to_string(),
                ]),
            },
            CargoLockPackage {
                name: "hyper".to_string(),
                version: "0.14.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["tokio 1.0.1".to_string()]),
            },
        ],
    };

    // Both serde and tokio change
    let directly_changed = vec!["serde".to_string(), "tokio".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    insta::assert_debug_snapshot!(result, @r###"
    [
        "hyper",
        "reqwest",
        "serde",
        "serde_json",
        "tokio",
    ]
    "###);
}

#[test]
fn test_no_dependencies() {
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            CargoLockPackage {
                name: "serde".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
            CargoLockPackage {
                name: "tokio".to_string(),
                version: "1.0.1".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: None,
            },
        ],
    };

    let directly_changed = vec!["serde".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    insta::assert_debug_snapshot!(result, @r###"
    [
        "serde",
    ]
    "###);
}

#[test]
fn test_circular_dependencies() {
    // Test edge case where there might be circular dependencies in the graph
    let cargo_lock = CargoLock {
        version: 3,
        package: vec![
            CargoLockPackage {
                name: "a".to_string(),
                version: "1.0.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["b 1.0.0".to_string()]),
            },
            CargoLockPackage {
                name: "b".to_string(),
                version: "1.0.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["c 1.0.0".to_string()]),
            },
            CargoLockPackage {
                name: "c".to_string(),
                version: "1.0.0".to_string(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".to_string()),
                dependencies: Some(vec!["a 1.0.0".to_string()]), // circular reference
            },
        ],
    };

    let directly_changed = vec!["a".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    // Should handle circular dependencies gracefully
    insta::assert_debug_snapshot!(result, @r###"
    [
        "a",
        "b",
        "c",
    ]
    "###);
}

#[test]
fn test_parse_cargo_lock_toml() {
    let cargo_lock_content = r#"
# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 3

[[package]]
name = "serde"
version = "1.0.195"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "63261df402c67811e9ac6def069e21a44217dcfe"
dependencies = [
    "serde_derive",
]

[[package]]
name = "serde_derive"
version = "1.0.195"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "46fe8f8603d81ba86327b23a2e9cdf49e1255fb94a4c5f297f6ee0547178ea2c"

[[package]]
name = "serde_json"
version = "1.0.111"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "176e46fa42316f18edd598015a5166857fc835ec732f5215eac6b7bdbf0a84f4"
dependencies = [
    "itoa",
    "ryu",
    "serde",
]
"#;

    let cargo_lock = parse_cargo_lock(cargo_lock_content).expect("Failed to parse Cargo.lock");

    insta::assert_debug_snapshot!(cargo_lock, @r###"
    CargoLock {
        version: 3,
        package: [
            CargoLockPackage {
                name: "serde",
                version: "1.0.195",
                source: Some(
                    "registry+https://github.com/rust-lang/crates.io-index",
                ),
                dependencies: Some(
                    [
                        "serde_derive",
                    ],
                ),
            },
            CargoLockPackage {
                name: "serde_derive",
                version: "1.0.195",
                source: Some(
                    "registry+https://github.com/rust-lang/crates.io-index",
                ),
                dependencies: None,
            },
            CargoLockPackage {
                name: "serde_json",
                version: "1.0.111",
                source: Some(
                    "registry+https://github.com/rust-lang/crates.io-index",
                ),
                dependencies: Some(
                    [
                        "itoa",
                        "ryu",
                        "serde",
                    ],
                ),
            },
        ],
    }
    "###);

    // Test transitive analysis with real Cargo.lock structure
    let directly_changed = vec!["serde".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    insta::assert_debug_snapshot!(result, @r###"
    [
        "serde",
        "serde_json",
    ]
    "###);
}
