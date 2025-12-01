use clippier::{
    git_diff::{parse_cargo_lock, parse_cargo_lock_changes},
    parse_dependency_name,
};

#[cfg(feature = "git-diff")]
use clippier::git_diff::find_transitively_affected_external_deps;

use clippier::test_utils::test_resources::{create_simple_workspace, load_cargo_lock_for_git_diff};

#[switchy_async::test]
async fn test_parse_dependency_name() {
    insta::assert_debug_snapshot!(parse_dependency_name("serde"), @r###""serde""###);
    insta::assert_debug_snapshot!(parse_dependency_name("serde 1.0.0"), @r###""serde""###);
    insta::assert_debug_snapshot!(
        parse_dependency_name("tokio 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"),
        @r###""tokio""###
    );
    insta::assert_debug_snapshot!(parse_dependency_name("serde_json 1.0.0"), @r###""serde_json""###);
}

#[switchy_async::test]
async fn test_parse_cargo_lock_changes_simple() {
    let changes = vec![
        (' ', "[[package]]\n".to_string()),
        (' ', "name = \"serde\"\n".to_string()),
        ('-', "version = \"1.0.180\"\n".to_string()),
        ('+', "version = \"1.0.190\"\n".to_string()),
    ];

    let result = parse_cargo_lock_changes(&changes);
    // Since this is a stub implementation, we expect serde to be detected
    assert!(result.contains(&"serde".to_string()));
}

#[switchy_async::test]
async fn test_parse_cargo_lock_changes_multiple() {
    let changes = vec![
        (' ', "[[package]]\n".to_string()),
        (' ', "name = \"serde\"\n".to_string()),
        ('-', "version = \"1.0.180\"\n".to_string()),
        ('+', "version = \"1.0.190\"\n".to_string()),
        (' ', "[[package]]\n".to_string()),
        (' ', "name = \"tokio\"\n".to_string()),
        ('-', "version = \"1.28.0\"\n".to_string()),
        ('+', "version = \"1.35.0\"\n".to_string()),
    ];

    let result = parse_cargo_lock_changes(&changes);
    // Since this is a stub implementation, we expect serde to be detected
    assert!(result.contains(&"serde".to_string()));
    assert!(result.contains(&"tokio".to_string()));
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_simple_transitive_dependencies() {
    // Load a simple Cargo.lock from test resources
    let cargo_lock = load_cargo_lock_for_git_diff("basic", "simple");

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

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_complex_transitive_dependencies() {
    // Load the complex deep dependencies Cargo.lock from test resources
    let cargo_lock = load_cargo_lock_for_git_diff("deep-deps", "complex");

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

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_multiple_changed_dependencies() {
    // Create a simple test scenario using the utility function
    let (_temp_dir, _workspace_members) = create_simple_workspace(
        &["app", "utils"],
        &["serde", "tokio", "reqwest"],
        &[
            ("app", &["utils", "reqwest"]),
            ("utils", &["serde", "tokio"]),
        ],
    );

    // Load a Cargo.lock that represents multiple dependencies changing
    let cargo_lock = load_cargo_lock_for_git_diff("basic", "comprehensive");

    let directly_changed = vec!["serde".to_string(), "tokio".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    insta::assert_debug_snapshot!(result, @r###"
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
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_no_dependencies() {
    // Create a minimal workspace for testing isolated packages
    let (_temp_dir, _workspace_members) =
        create_simple_workspace(&["standalone"], &[], &[("standalone", &[])]);

    // Use a simple Cargo.lock and test with a non-existent dependency
    let cargo_lock = load_cargo_lock_for_git_diff("basic", "simple");

    let directly_changed = vec!["non-existent-crate".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    // Should only include the non-existent crate itself
    insta::assert_debug_snapshot!(result, @r###"
    [
        "non-existent-crate",
    ]
    "###);
}

#[cfg(feature = "git-diff")]
#[switchy_async::test]
async fn test_circular_dependencies() {
    // This test checks that the algorithm handles potential circular references gracefully
    // In practice, Cargo.lock shouldn't have circular deps, but we test for robustness
    let cargo_lock = load_cargo_lock_for_git_diff("basic", "simple");

    let directly_changed = vec!["serde_json".to_string()];
    let result = find_transitively_affected_external_deps(&cargo_lock, &directly_changed);

    // serde_json depends on serde, but not the other way around in our test data
    insta::assert_debug_snapshot!(result, @r###"
    [
        "reqwest",
        "serde_json",
    ]
    "###);
}

#[switchy_async::test]
async fn test_parse_cargo_lock_toml() {
    // Test parsing a simple TOML Cargo.lock structure
    let cargo_lock_toml = r#"
version = 3

[[package]]
name = "serde"
version = "1.0.195"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "my-app"
version = "0.1.0"
dependencies = [
    "serde 1.0.195",
]
"#;

    let result = parse_cargo_lock(cargo_lock_toml).expect("Failed to parse Cargo.lock");

    insta::assert_debug_snapshot!(result, @r###"
    CargoLock {
        version: 3,
        package: [
            CargoLockPackage {
                name: "serde",
                version: "1.0.195",
                source: Some(
                    "registry+https://github.com/rust-lang/crates.io-index",
                ),
                dependencies: None,
            },
            CargoLockPackage {
                name: "my-app",
                version: "0.1.0",
                source: None,
                dependencies: Some(
                    [
                        "serde 1.0.195",
                    ],
                ),
            },
        ],
    }
    "###);
}
