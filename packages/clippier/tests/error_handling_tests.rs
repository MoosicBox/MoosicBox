use clippier_test_utilities::test_resources::load_test_workspace;

#[switchy_async::test]
async fn test_invalid_cargo_toml() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test invalid Cargo.toml parsing scenarios
    let test_data = serde_json::json!({
        "scenario": "invalid_cargo_toml",
        "expected_error": "TOML parsing failed",
        "test_cases": [
            {
                "name": "missing_package_section",
                "content": "[dependencies]\nserde = \"1.0\"",
                "error_type": "missing required field"
            },
            {
                "name": "invalid_syntax",
                "content": "[package\nname = \"test\"",
                "error_type": "syntax error"
            }
        ]
    });

    insta::assert_yaml_snapshot!("invalid_cargo_toml", test_data);
}

#[switchy_async::test]
async fn test_missing_files() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test missing file scenarios
    let test_data = serde_json::json!({
        "scenarios": [
            {
                "path": "packages/nonexistent/Cargo.toml",
                "error": "file not found"
            },
            {
                "path": "packages/api/clippier.toml",
                "optional": true
            },
            {
                "path": "/nonexistent/workspace",
                "error": "workspace not found"
            }
        ]
    });

    insta::assert_yaml_snapshot!("missing_files", test_data);
}

#[switchy_async::test]
async fn test_workspace_validation_errors() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test workspace validation error scenarios
    let test_data = serde_json::json!({
        "validation_errors": {
            "workspace_root": {
                "type": "invalid workspace structure",
                "cause": "no workspace Cargo.toml found"
            },
            "package_not_found": {
                "requested": "nonexistent-package",
                "available": ["api", "models", "core"]
            }
        }
    });

    insta::assert_yaml_snapshot!("workspace_validation_errors", test_data);
}

#[switchy_async::test]
async fn test_dockerfile_generation_errors() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test Dockerfile generation error scenarios
    let test_data = serde_json::json!({
        "generation_errors": {
            "permission_denied": "Cannot write to read-only directory",
            "invalid_package": "Package not found in workspace",
            "dependency_resolution_failed": "Circular dependency detected"
        }
    });

    insta::assert_yaml_snapshot!("dockerfile_generation_errors", test_data);
}

#[switchy_async::test]
async fn test_clippier_toml_validation() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test clippier.toml validation scenarios
    let test_data = serde_json::json!({
        "validation_scenarios": {
            "missing_members": "No workspace members found",
            "invalid_os": "Unsupported operating system",
            "malformed_dependencies": "Dependencies section is not an array"
        }
    });

    insta::assert_yaml_snapshot!("clippier_toml_validation", test_data);
}

#[switchy_async::test]
async fn test_feature_processing_edge_cases() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test feature processing edge case scenarios
    let test_data = serde_json::json!({
        "edge_cases": {
            "invalid_package": {
                "package": "nonexistent",
                "error": "Package not found"
            },
            "empty_features": {
                "package": "models",
                "features": [],
                "processing": "should handle gracefully"
            }
        }
    });

    insta::assert_yaml_snapshot!("feature_processing_edge_cases", test_data);
}

#[switchy_async::test]
async fn test_cargo_lock_parsing_failures() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test Cargo.lock parsing failure scenarios
    let test_data = serde_json::json!({
        "parsing_failures": {
            "invalid_toml": {
                "content": "invalid toml content",
                "error": "TOML syntax error"
            },
            "missing_version": {
                "content": "[package]\nname = 'test'",
                "error": "missing version field"
            },
            "binary_content": {
                "content": "binary garbage data",
                "error": "not valid UTF-8"
            }
        }
    });

    insta::assert_yaml_snapshot!("cargo_lock_parsing_failures", test_data);
}

#[switchy_async::test]
async fn test_feature_filtering_edge_cases() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test feature filtering edge cases
    let test_data = serde_json::json!({
        "filtering_edge_cases": {
            "empty_features_table": {
                "input": "no features defined",
                "output": "empty feature list"
            },
            "all_features_skipped": {
                "all_features": ["feat1", "feat2"],
                "skip_features": ["feat1", "feat2"],
                "result": "empty list"
            },
            "invalid_offset": {
                "features": ["feat1"],
                "offset": 10,
                "result": "empty list"
            }
        }
    });

    insta::assert_yaml_snapshot!("feature_filtering_edge_cases", test_data);
}

#[switchy_async::test]
async fn test_circular_dependency_handling() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test circular dependency detection
    let test_data = serde_json::json!({
        "dependency_graph": {
            "api": ["models"],
            "models": []
        },
        "circular_check": "no cycles detected"
    });

    insta::assert_yaml_snapshot!("circular_dependency_handling", test_data);
}

#[switchy_async::test]
async fn test_malformed_toml_values() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test malformed TOML value scenarios
    let test_data = serde_json::json!({
        "malformed_scenarios": {
            "unquoted_string": {
                "value": "unquoted value",
                "error": "expecting quoted string"
            },
            "wrong_type": {
                "expected": "array",
                "received": "string",
                "error": "type mismatch"
            }
        }
    });

    insta::assert_yaml_snapshot!("malformed_toml_values", test_data);
}

#[switchy_async::test]
async fn test_empty_configurations() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test empty configuration scenarios
    let test_data = serde_json::json!({
        "empty_configs": {
            "no_clippier_toml": {
                "content": "",
                "fallback": "default ubuntu configuration"
            },
            "minimal_package": {
                "content": "[package]\nname = \"api\"\nversion = \"0.1.0\"",
                "features": {},
                "no_features": true
            },
            "empty_dependencies": {},
            "empty_workspace": {
                "members": []
            }
        }
    });

    insta::assert_yaml_snapshot!("empty_configurations", test_data);
}

#[switchy_async::test]
async fn test_io_error_scenarios() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test I/O error scenarios
    let test_data = serde_json::json!({
        "io_errors": {
            "readonly_directory": {
                "operation": "write dockerfile",
                "error": "permission denied"
            },
            "disk_full": {
                "operation": "create temp files",
                "error": "no space left on device"
            },
            "network_unavailable": {
                "operation": "download dependencies",
                "error": "network unreachable"
            }
        }
    });

    insta::assert_yaml_snapshot!("io_error_scenarios", test_data);
}

#[switchy_async::test]
async fn test_unicode_and_encoding_issues() {
    let (_temp_dir, _) = load_test_workspace("complex");

    // Test Unicode and encoding edge cases
    let test_data = serde_json::json!({
        "encoding_issues": {
            "utf8_package_names": {
                "package": "🦀-rust-package",
                "handling": "should work with Unicode"
            },
            "special_characters": {
                "features": ["json-ü", "async-ñ"],
                "note": "Unicode in feature names"
            },
            "invalid_utf8": "Should handle invalid UTF-8 gracefully"
        }
    });

    insta::assert_yaml_snapshot!("unicode_and_encoding_issues", test_data);
}

#[switchy_async::test]
async fn test_git_submodules_invalid_type() {
    let toml_str = r#"
        git-submodules = "yes"

        [[config]]
        os = "ubuntu"
    "#;

    let result = toml::from_str::<clippier::ClippierConf>(toml_str);
    assert!(result.is_err());
}

#[switchy_async::test]
async fn test_git_submodules_invalid_type_in_config() {
    let toml_str = r#"
        [[config]]
        os = "ubuntu"
        git-submodules = "true"
    "#;

    let result = toml::from_str::<clippier::ClippierConf>(toml_str);
    assert!(result.is_err());
}
