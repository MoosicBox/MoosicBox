use clippier::{find_affected_packages, find_affected_packages_with_reasoning};
use clippier_test_utilities::test_resources::load_test_workspace;

#[switchy_async::test]
async fn test_ignore_markdown_files_single_pattern() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/README.md".to_string()];
    let ignore_patterns = vec!["**/*.md".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages.len(),
        0,
        "README.md should be ignored, no packages affected"
    );
}

#[switchy_async::test]
async fn test_ignore_multiple_file_types() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/api/README.md".to_string(),
        "packages/web/CHANGELOG.txt".to_string(),
        "packages/core/notes.md".to_string(),
    ];
    let ignore_patterns = vec!["**/*.md".to_string(), "**/*.txt".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages.len(),
        0,
        "All documentation files should be ignored"
    );
}

#[switchy_async::test]
async fn test_ignore_patterns_dont_affect_code_files() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/core/src/lib.rs".to_string(),
    ];
    let ignore_patterns = vec!["**/*.md".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["core"],
        "Code change should still be detected despite ignore pattern"
    );
}

#[switchy_async::test]
async fn test_ignore_with_negation_pattern() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/api/IMPORTANT.md".to_string(),
    ];
    let ignore_patterns = vec!["**/*.md".to_string(), "!**/IMPORTANT.md".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["api"],
        "IMPORTANT.md should trigger detection despite wildcard ignore"
    );
}

#[switchy_async::test]
async fn test_ignore_nested_directory_files() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/docs/api.md".to_string(),
        "packages/web/guides/tutorial.md".to_string(),
    ];
    let ignore_patterns = vec!["**/docs/**/*.md".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["web"],
        "Only web should be affected, core/docs ignored"
    );
}

#[switchy_async::test]
async fn test_empty_ignore_patterns_detects_all() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/README.md".to_string()];
    let ignore_patterns: Vec<String> = vec![];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["core"],
        "Without ignore patterns, .md files should affect packages"
    );
}

#[switchy_async::test]
async fn test_mixed_ignored_and_detected_files() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/api/notes.txt".to_string(),
        "packages/web/src/lib.rs".to_string(),
        "packages/cli/Cargo.toml".to_string(),
    ];
    let ignore_patterns = vec!["**/*.md".to_string(), "**/*.txt".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["cli", "web"],
        "Only code/config changes should trigger detection"
    );
}

#[switchy_async::test]
async fn test_ignore_patterns_with_reasoning() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/api/src/lib.rs".to_string(),
    ];
    let ignore_patterns = vec!["**/*.md".to_string()];

    let result =
        find_affected_packages_with_reasoning(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(packages.len(), 1, "Only api should be affected");
    assert_eq!(packages[0].name, "api");

    if let Some(reasoning) = &packages[0].reasoning {
        let reasoning_str = reasoning.join(" ");
        assert!(
            reasoning_str.contains("src/lib.rs"),
            "Reasoning should mention the code file"
        );
        assert!(
            !reasoning_str.contains("README.md"),
            "Reasoning should not mention ignored files"
        );
    }
}

#[switchy_async::test]
async fn test_ignore_pattern_evaluation_order() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec!["packages/core/CRITICAL.md".to_string()];
    let ignore_patterns = vec!["!**/CRITICAL.md".to_string(), "**/*.md".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages.len(),
        0,
        "Later patterns should override earlier ones (like GitHub Actions)"
    );
}

#[switchy_async::test]
async fn test_ignore_specific_extensions() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/Dockerfile".to_string(),
        "packages/api/Server.Dockerfile".to_string(),
        "packages/web/.dockerignore".to_string(),
    ];
    let ignore_patterns = vec![
        "**/Dockerfile".to_string(),
        "**/*.Dockerfile".to_string(),
        "**/*.dockerignore".to_string(),
    ];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages.len(),
        0,
        "All Docker-related files should be ignored"
    );
}

#[switchy_async::test]
async fn test_ignore_workflow_patterns() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/api/notes.txt".to_string(),
        "packages/web/Dockerfile".to_string(),
        "packages/cli/Server.Dockerfile".to_string(),
        "packages/models/.dockerignore".to_string(),
        "packages/shared-utils/flake.nix".to_string(),
    ];
    let ignore_patterns = vec![
        "**/*.md".to_string(),
        "**/*.txt".to_string(),
        "**/Dockerfile".to_string(),
        "**/*.Dockerfile".to_string(),
        "**/*.dockerignore".to_string(),
        "**/*.nix".to_string(),
    ];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages.len(),
        0,
        "All workflow ignore patterns should work correctly"
    );
}

#[switchy_async::test]
async fn test_ignore_only_some_packages() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/api/src/lib.rs".to_string(),
        "packages/web/notes.txt".to_string(),
    ];
    let ignore_patterns = vec!["**/*.md".to_string(), "**/*.txt".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["api"],
        "Only api should be affected by code change"
    );
}

#[switchy_async::test]
async fn test_ignore_with_code_and_docs_mixed() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/src/lib.rs".to_string(),
        "packages/models/README.md".to_string(),
        "packages/api/docs/guide.md".to_string(),
    ];
    let ignore_patterns = vec!["**/*.md".to_string()];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["core"],
        "Only core should be affected by code change, .md files ignored"
    );
}

#[switchy_async::test]
async fn test_negation_pattern_with_specific_file() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/api/docs/README.md".to_string(),
        "packages/web/README.md".to_string(),
    ];
    let ignore_patterns = vec![
        "**/*.md".to_string(),
        "!packages/api/docs/README.md".to_string(),
    ];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages,
        vec!["api"],
        "Only api should be affected by un-ignored README.md"
    );
}

#[switchy_async::test]
async fn test_multiple_negation_patterns() {
    let (temp_dir, _) = load_test_workspace("complex");
    let changed_files = vec![
        "packages/core/README.md".to_string(),
        "packages/api/IMPORTANT.md".to_string(),
        "packages/web/CRITICAL.md".to_string(),
        "packages/cli/notes.md".to_string(),
    ];
    let ignore_patterns = vec![
        "**/*.md".to_string(),
        "!**/IMPORTANT.md".to_string(),
        "!**/CRITICAL.md".to_string(),
    ];

    let result = find_affected_packages(temp_dir.path(), &changed_files, &ignore_patterns);

    assert!(result.is_ok());
    let packages = result.unwrap();
    assert_eq!(
        packages.len(),
        2,
        "Api and web should be affected by un-ignored files"
    );
    assert!(packages.contains(&"api".to_string()));
    assert!(packages.contains(&"web".to_string()));
    assert!(!packages.contains(&"core".to_string()));
    assert!(!packages.contains(&"cli".to_string()));
}
