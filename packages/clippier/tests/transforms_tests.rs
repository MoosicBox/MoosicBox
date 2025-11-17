#![cfg(feature = "_transforms")]

use clippier::transforms::{TransformContext, TransformEngine};
use serde_json::json;

/// Helper to get the path to test fixtures
fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/transforms/fixtures")
        .join(name)
}

/// Helper to get the path to test scripts
fn script_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/transforms/scripts")
        .join(name)
}

/// Debug helper: Pretty-print matrix contents to stderr
#[allow(dead_code)]
fn debug_matrix(matrix: &[serde_json::Map<String, serde_json::Value>], label: &str) {
    eprintln!("\n╔═══ {} ═══", label);
    eprintln!("║ Matrix size: {} entries", matrix.len());
    eprintln!("╠═══════════════════════════════════════");
    for (i, entry) in matrix.iter().enumerate() {
        let pkg = entry.get("package").and_then(|v| v.as_str()).unwrap_or("?");
        let os = entry.get("os").and_then(|v| v.as_str()).unwrap_or("?");
        let features = entry
            .get("features")
            .map(|v| format!("{}", v))
            .or_else(|| entry.get("feature").map(|v| format!("\"{}\"", v)))
            .unwrap_or_else(|| "[]".to_string());
        eprintln!("║ [{:2}] {:15} on {:10} with {}", i, pkg, os, features);
    }
    eprintln!("╚═══ End {} ═══\n", label);
}

/// Debug helper: Assert matrix contains specific entry
#[allow(dead_code)]
fn assert_matrix_contains(
    matrix: &[serde_json::Map<String, serde_json::Value>],
    package: &str,
    os: &str,
    expected_features: &[&str],
) {
    let found = matrix.iter().find(|e| {
        e.get("package").and_then(|v| v.as_str()) == Some(package)
            && e.get("os").and_then(|v| v.as_str()) == Some(os)
    });

    match found {
        Some(entry) => {
            if let Some(features_val) = entry.get("features") {
                let actual_features: Vec<String> = serde_json::from_value(features_val.clone())
                    .expect("Failed to parse features array");
                let expected: Vec<String> =
                    expected_features.iter().map(|s| s.to_string()).collect();
                assert_eq!(
                    actual_features, expected,
                    "Package '{}' on '{}' has wrong features.\nExpected: {:?}\nActual: {:?}",
                    package, os, expected, actual_features
                );
            } else if let Some(feature_val) = entry.get("feature") {
                let actual_feature = feature_val.as_str().expect("Feature should be string");
                assert_eq!(
                    expected_features.len(),
                    1,
                    "Expected single feature in legacy format"
                );
                assert_eq!(
                    actual_feature, expected_features[0],
                    "Package '{}' on '{}' has wrong feature",
                    package, os
                );
            } else {
                panic!(
                    "Package '{}' on '{}' found but has no features or feature field",
                    package, os
                );
            }
        }
        None => {
            eprintln!(
                "\n❌ Package '{}' on '{}' NOT FOUND in matrix!",
                package, os
            );
            eprintln!("Available entries:");
            debug_matrix(matrix, "Current Matrix");
            panic!(
                "Assertion failed: Package '{}' on '{}' not found",
                package, os
            );
        }
    }
}

/// Debug helper: Count matrix entries matching criteria
#[allow(dead_code)]
fn count_matrix_entries(
    matrix: &[serde_json::Map<String, serde_json::Value>],
    package: Option<&str>,
    os: Option<&str>,
) -> usize {
    matrix
        .iter()
        .filter(|e| {
            let pkg_match =
                package.is_none_or(|p| e.get("package").and_then(|v| v.as_str()) == Some(p));
            let os_match = os.is_none_or(|o| e.get("os").and_then(|v| v.as_str()) == Some(o));
            pkg_match && os_match
        })
        .count()
}

#[test]
fn test_transform_context_new() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    // Should have loaded all 3 packages
    let packages = context.get_all_packages();
    assert_eq!(packages.len(), 3);
    assert!(packages.contains(&"api".to_string()));
    assert!(packages.contains(&"models".to_string()));
    assert!(packages.contains(&"client".to_string()));
}

#[test]
fn test_context_get_package() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    // Test get_package
    let api = context.get_package("api");
    assert!(api.is_some());
    let api = api.unwrap();
    assert_eq!(api.name, "api");

    // Test non-existent package
    let missing = context.get_package("nonexistent");
    assert!(missing.is_none());
}

#[test]
fn test_context_is_workspace_member() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    assert!(context.is_workspace_member("api"));
    assert!(context.is_workspace_member("models"));
    assert!(context.is_workspace_member("client"));
    assert!(!context.is_workspace_member("serde"));
    assert!(!context.is_workspace_member("tokio"));
}

#[test]
fn test_context_package_depends_on() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    // api depends on models
    assert!(context.package_depends_on("api", "models"));

    // client depends on api
    assert!(context.package_depends_on("client", "api"));

    // models doesn't depend on api
    assert!(!context.package_depends_on("models", "api"));

    // api depends on external deps
    assert!(context.package_depends_on("api", "serde"));
    assert!(context.package_depends_on("api", "tokio"));
}

#[test]
fn test_context_feature_exists() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    assert!(context.feature_exists("api", "async"));
    assert!(context.feature_exists("api", "json"));
    assert!(context.feature_exists("models", "serialization"));
    assert!(!context.feature_exists("api", "nonexistent"));
}

#[test]
fn test_package_info_depends_on() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    let api = context.get_package("api").unwrap();
    assert!(api.depends_on("models"));
    assert!(api.depends_on("serde"));
    assert!(api.depends_on("tokio"));
    assert!(!api.depends_on("reqwest"));
}

#[test]
fn test_package_info_has_feature() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    let api = context.get_package("api").unwrap();
    assert!(api.has_feature("async"));
    assert!(api.has_feature("json"));
    assert!(api.has_feature("default"));
    assert!(!api.has_feature("nonexistent"));
}

#[test]
fn test_package_info_get_all_features() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    let api = context.get_package("api").unwrap();
    let features = api.get_all_features();

    assert!(features.contains(&"async".to_string()));
    assert!(features.contains(&"json".to_string()));
    assert!(features.contains(&"default".to_string()));
}

#[test]
fn test_package_info_feature_activates_dependencies() {
    let workspace_root = fixture_path("basic");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    let api = context.get_package("api").unwrap();

    // async feature activates tokio/macros
    let async_deps = api.feature_activates_dependencies("async");
    assert_eq!(async_deps.len(), 1);
    assert_eq!(async_deps[0].name, "tokio");
    assert_eq!(async_deps[0].features, vec!["macros"]);

    // json feature activates serde/derive
    let json_deps = api.feature_activates_dependencies("json");
    assert_eq!(json_deps.len(), 1);
    assert_eq!(json_deps[0].name, "serde");
    assert_eq!(json_deps[0].features, vec!["derive"]);
}

#[test]
fn test_package_info_skips_feature_on_os() {
    // Note: The asio-example fixture no longer uses clippier.toml for skip-features
    // because the transform handles it via dependency graph analysis.
    // This test now verifies that skips_feature_on_os returns false when no config exists
    let workspace_root = fixture_path("asio-example");
    let context = TransformContext::new(&workspace_root).expect("Failed to create context");

    let audio_output = context.get_package("audio_output").unwrap();

    // No clippier.toml config, so should return false
    assert!(!audio_output.skips_feature_on_os("asio", "windows"));
    assert!(!audio_output.skips_feature_on_os("asio", "linux"));
    assert!(!audio_output.skips_feature_on_os("default", "windows"));

    // Test with basic fixture that might have config
    let basic_root = fixture_path("basic");
    let basic_context = TransformContext::new(&basic_root).expect("Failed to create context");
    let api = basic_context.get_package("api").unwrap();

    // basic fixture also has no clippier.toml
    assert!(!api.skips_feature_on_os("async", "windows"));
}

#[test]
fn test_transform_engine_new() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    // Engine should be created successfully - we can't inspect it much
    // but we can test that it works
    let mut matrix = vec![serde_json::Map::from_iter(vec![
        ("package".to_string(), json!("api")),
        ("feature".to_string(), json!("async")),
    ])];

    // Simple identity transform
    let script = r"
        function transform(context, matrix)
            return matrix
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix.len(), 1);
}

#[test]
fn test_apply_transform_basic() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("api")),
            ("feature".to_string(), json!("async")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("models")),
            ("feature".to_string(), json!("serialization")),
        ]),
    ];

    // Filter out api package
    let script = r"
        function transform(context, matrix)
            return table.filter(matrix, function(entry)
                return entry.package ~= 'api'
            end)
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix.len(), 1);
    assert_eq!(matrix[0].get("package").unwrap(), "models");
}

#[test]
fn test_lua_helper_table_filter() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![
        serde_json::Map::from_iter(vec![("value".to_string(), json!(1))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(2))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(3))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(4))]),
    ];

    let script = r"
        function transform(context, matrix)
            return table.filter(matrix, function(entry)
                return entry.value % 2 == 0
            end)
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix.len(), 2);
    assert_eq!(matrix[0].get("value").unwrap(), 2);
    assert_eq!(matrix[1].get("value").unwrap(), 4);
}

#[test]
fn test_lua_helper_table_map() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![
        serde_json::Map::from_iter(vec![("value".to_string(), json!(1))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(2))]),
    ];

    let script = r"
        function transform(context, matrix)
            return table.map(matrix, function(entry)
                entry.doubled = entry.value * 2
                return entry
            end)
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix.len(), 2);
    assert_eq!(matrix[0].get("doubled").unwrap(), 2);
    assert_eq!(matrix[1].get("doubled").unwrap(), 4);
}

#[test]
fn test_lua_helper_table_contains() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![serde_json::Map::from_iter(vec![(
        "package".to_string(),
        json!("api"),
    )])];

    let script = r"
        function transform(context, matrix)
            local packages = context:get_all_packages()
            local has_api = table.contains(packages, 'api')
            local has_missing = table.contains(packages, 'missing')

            matrix[1].has_api = has_api
            matrix[1].has_missing = has_missing
            return matrix
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix[0].get("has_api").unwrap(), true);
    assert_eq!(matrix[0].get("has_missing").unwrap(), false);
}

#[test]
fn test_lua_helper_table_find() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![
        serde_json::Map::from_iter(vec![("value".to_string(), json!(1))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(2))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(3))]),
    ];

    let script = r"
        function transform(context, matrix)
            local found = table.find(matrix, function(entry)
                return entry.value == 2
            end)

            return {found}
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix.len(), 1);
    assert_eq!(matrix[0].get("value").unwrap(), 2);
}

#[test]
fn test_lua_context_api_get_package() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![serde_json::Map::from_iter(vec![(
        "test".to_string(),
        json!(true),
    )])];

    let script = r"
        function transform(context, matrix)
            local pkg = context:get_package('api')
            matrix[1].package_name = pkg.name
            matrix[1].has_async = pkg:has_feature('async')
            return matrix
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix[0].get("package_name").unwrap(), "api");
    assert_eq!(matrix[0].get("has_async").unwrap(), true);
}

#[test]
fn test_lua_context_api_is_workspace_member() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![serde_json::Map::from_iter(vec![(
        "test".to_string(),
        json!(true),
    )])];

    let script = r"
        function transform(context, matrix)
            matrix[1].api_is_member = context:is_workspace_member('api')
            matrix[1].serde_is_member = context:is_workspace_member('serde')
            return matrix
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix[0].get("api_is_member").unwrap(), true);
    assert_eq!(matrix[0].get("serde_is_member").unwrap(), false);
}

#[test]
fn test_lua_context_api_package_depends_on() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![serde_json::Map::from_iter(vec![(
        "test".to_string(),
        json!(true),
    )])];

    let script = r"
        function transform(context, matrix)
            matrix[1].api_depends_models = context:package_depends_on('api', 'models')
            matrix[1].models_depends_api = context:package_depends_on('models', 'api')
            return matrix
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix[0].get("api_depends_models").unwrap(), true);
    assert_eq!(matrix[0].get("models_depends_api").unwrap(), false);
}

#[test]
fn test_lua_context_api_feature_exists() {
    let workspace_root = fixture_path("basic");
    let engine = TransformEngine::new(&workspace_root).expect("Failed to create engine");

    let mut matrix = vec![serde_json::Map::from_iter(vec![(
        "test".to_string(),
        json!(true),
    )])];

    let script = r"
        function transform(context, matrix)
            matrix[1].async_exists = context:feature_exists('api', 'async')
            matrix[1].missing_exists = context:feature_exists('api', 'missing')
            return matrix
        end
    ";

    engine
        .apply_transform(&mut matrix, script)
        .expect("Transform failed");
    assert_eq!(matrix[0].get("async_exists").unwrap(), true);
    assert_eq!(matrix[0].get("missing_exists").unwrap(), false);
}

#[test]
fn test_inline_script_loading() {
    let workspace_root = fixture_path("basic");
    let mut matrix = vec![serde_json::Map::from_iter(vec![(
        "value".to_string(),
        json!(1),
    )])];

    let inline_script = r"
        function transform(context, matrix)
            matrix[1].doubled = matrix[1].value * 2
            return matrix
        end
    ";

    clippier::transforms::apply_transforms(
        &mut matrix,
        &[inline_script.to_string()],
        &workspace_root,
    )
    .expect("Transform failed");

    assert_eq!(matrix[0].get("doubled").unwrap(), 2);
}

#[test]
fn test_file_script_loading() {
    let workspace_root = fixture_path("basic");
    let script_file = script_path("platform-filter.lua");

    let mut matrix = vec![
        serde_json::Map::from_iter(vec![
            ("os".to_string(), json!("windows")),
            ("feature".to_string(), json!("asio")),
        ]),
        serde_json::Map::from_iter(vec![
            ("os".to_string(), json!("linux")),
            ("feature".to_string(), json!("asio")),
        ]),
    ];

    clippier::transforms::apply_transforms(
        &mut matrix,
        &[script_file.to_string_lossy().to_string()],
        &workspace_root,
    )
    .expect("Transform failed");

    // Windows + ASIO should be filtered out by platform-filter.lua
    assert_eq!(matrix.len(), 1);
    assert_eq!(matrix[0].get("os").unwrap(), "linux");
}

#[test]
fn test_multiple_transforms_in_sequence() {
    let workspace_root = fixture_path("basic");

    let mut matrix = vec![
        serde_json::Map::from_iter(vec![("value".to_string(), json!(1))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(2))]),
        serde_json::Map::from_iter(vec![("value".to_string(), json!(3))]),
    ];

    let transform1 = r"
        function transform(context, matrix)
            return table.filter(matrix, function(e) return e.value > 1 end)
        end
    ";

    let transform2 = r"
        function transform(context, matrix)
            return table.map(matrix, function(e)
                e.doubled = e.value * 2
                return e
            end)
        end
    ";

    clippier::transforms::apply_transforms(
        &mut matrix,
        &[transform1.to_string(), transform2.to_string()],
        &workspace_root,
    )
    .expect("Transform failed");

    assert_eq!(matrix.len(), 2);
    assert_eq!(matrix[0].get("value").unwrap(), 2);
    assert_eq!(matrix[0].get("doubled").unwrap(), 4);
    assert_eq!(matrix[1].get("value").unwrap(), 3);
    assert_eq!(matrix[1].get("doubled").unwrap(), 6);
}

#[test]
fn test_error_script_compilation() {
    let workspace_root = fixture_path("basic");
    let mut matrix = vec![];

    let bad_script = r"
        function transform(context, matrix)
            this is not valid lua syntax !!!
        end
    ";

    let result = clippier::transforms::apply_transforms(
        &mut matrix,
        &[bad_script.to_string()],
        &workspace_root,
    );

    assert!(result.is_err());
}

#[test]
fn test_error_script_runtime() {
    let workspace_root = fixture_path("basic");
    let mut matrix = vec![];

    let bad_script = r"
        function transform(context, matrix)
            error('Intentional runtime error')
        end
    ";

    let result = clippier::transforms::apply_transforms(
        &mut matrix,
        &[bad_script.to_string()],
        &workspace_root,
    );

    assert!(result.is_err());
}

#[test]
fn test_error_missing_transform_function() {
    let workspace_root = fixture_path("basic");
    let mut matrix = vec![];

    let bad_script = r"
        -- No transform function defined
        local x = 1
    ";

    let result = clippier::transforms::apply_transforms(
        &mut matrix,
        &[bad_script.to_string()],
        &workspace_root,
    );

    assert!(result.is_err());
}

#[test]
fn test_asio_platform_compat_real_world() {
    let workspace_root = fixture_path("asio-example");
    let script_file = script_path("asio-compat.lua");

    // Real-world scenario: Transitive dependency activation
    // server/full -> player/asio -> audio_output/asio -> cpal/asio (requires ASIO SDK on Windows!)
    let mut matrix = vec![
        // Server package with "full" feature that transitively enables ASIO
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("server")),
            ("features".to_string(), json!(["audio", "full"])),
            ("os".to_string(), json!("windows")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("server")),
            ("features".to_string(), json!(["audio", "full"])),
            ("os".to_string(), json!("linux")),
        ]),
        // Player package with direct asio feature
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("player")),
            ("features".to_string(), json!(["asio"])),
            ("os".to_string(), json!("windows")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("player")),
            ("features".to_string(), json!(["asio"])),
            ("os".to_string(), json!("linux")),
        ]),
        // Audio_output package with direct asio feature
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("audio_output")),
            ("features".to_string(), json!(["default", "asio"])),
            ("os".to_string(), json!("windows")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("audio_output")),
            ("features".to_string(), json!(["default", "asio"])),
            ("os".to_string(), json!("linux")),
        ]),
    ];

    clippier::transforms::apply_transforms(
        &mut matrix,
        &[script_file.to_string_lossy().to_string()],
        &workspace_root,
    )
    .expect("Transform failed");

    // Windows player entry should be completely removed (all features filtered)
    // Final count: 5 entries (6 original - 1 Windows player removed)
    assert_eq!(matrix.len(), 5);

    // [0] Windows server: "full" feature removed (transitively enables asio), "audio" remains
    assert_eq!(matrix[0].get("package").unwrap(), "server");
    assert_eq!(matrix[0].get("os").unwrap(), "windows");
    assert_eq!(matrix[0].get("features").unwrap(), &json!(["audio"]));

    // [1] Linux server: both features remain
    assert_eq!(matrix[1].get("package").unwrap(), "server");
    assert_eq!(matrix[1].get("os").unwrap(), "linux");
    assert_eq!(
        matrix[1].get("features").unwrap(),
        &json!(["audio", "full"])
    );

    // [2] Linux player: asio feature remains (only problematic on Windows)
    assert_eq!(matrix[2].get("package").unwrap(), "player");
    assert_eq!(matrix[2].get("os").unwrap(), "linux");
    assert_eq!(matrix[2].get("features").unwrap(), &json!(["asio"]));

    // [3] Windows audio_output: asio removed, default remains
    assert_eq!(matrix[3].get("package").unwrap(), "audio_output");
    assert_eq!(matrix[3].get("os").unwrap(), "windows");
    assert_eq!(matrix[3].get("features").unwrap(), &json!(["default"]));

    // [4] Linux audio_output: both features remain
    assert_eq!(matrix[4].get("package").unwrap(), "audio_output");
    assert_eq!(matrix[4].get("os").unwrap(), "linux");
    assert_eq!(
        matrix[4].get("features").unwrap(),
        &json!(["default", "asio"])
    );
}

#[test]
fn test_asio_compat_single_feature_legacy() {
    let workspace_root = fixture_path("asio-example");
    let script_file = script_path("asio-compat.lua");

    // Legacy format: single feature field instead of features array
    // Test direct asio feature on audio_output (not transitive)
    let mut matrix = vec![
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("audio_output")),
            ("feature".to_string(), json!("asio")),
            ("os".to_string(), json!("windows")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("audio_output")),
            ("feature".to_string(), json!("asio")),
            ("os".to_string(), json!("linux")),
        ]),
        // Test transitive activation through player
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("player")),
            ("feature".to_string(), json!("asio")),
            ("os".to_string(), json!("windows")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("player")),
            ("feature".to_string(), json!("asio")),
            ("os".to_string(), json!("linux")),
        ]),
    ];

    clippier::transforms::apply_transforms(
        &mut matrix,
        &[script_file.to_string_lossy().to_string()],
        &workspace_root,
    )
    .expect("Transform failed");

    // Both Windows entries should be removed (direct asio on audio_output, transitive through player)
    // Linux entries should remain
    // However, player on Linux is at index 1, not 0
    assert_eq!(matrix.len(), 2, "Expected 2 Linux entries to remain");

    // Entries might be in different order, check both
    let packages: Vec<_> = matrix
        .iter()
        .map(|e| e.get("package").unwrap().as_str().unwrap())
        .collect();
    assert!(packages.contains(&"audio_output"));
    assert!(packages.contains(&"player"));

    for entry in &matrix {
        assert_eq!(
            entry.get("os").unwrap(),
            "linux",
            "Only Linux entries should remain"
        );
        assert_eq!(entry.get("feature").unwrap(), "asio");
    }
}

#[test]
fn test_asio_compat_all_features_filtered() {
    let workspace_root = fixture_path("asio-example");
    let script_file = script_path("asio-compat.lua");

    // Entry with ONLY asio-enabling features on Windows - should be completely removed
    let mut matrix = vec![
        // audio_output with only asio - should be removed on Windows
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("audio_output")),
            ("features".to_string(), json!(["asio"])),
            ("os".to_string(), json!("windows")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("audio_output")),
            ("features".to_string(), json!(["asio"])),
            ("os".to_string(), json!("linux")),
        ]),
        // server with only "full" which transitively enables asio - should be removed on Windows
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("server")),
            ("features".to_string(), json!(["full"])),
            ("os".to_string(), json!("windows")),
        ]),
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("server")),
            ("features".to_string(), json!(["full"])),
            ("os".to_string(), json!("linux")),
        ]),
        // audio_output with default + asio - should keep default only on Windows
        serde_json::Map::from_iter(vec![
            ("package".to_string(), json!("audio_output")),
            ("features".to_string(), json!(["default", "asio"])),
            ("os".to_string(), json!("windows")),
        ]),
    ];

    clippier::transforms::apply_transforms(
        &mut matrix,
        &[script_file.to_string_lossy().to_string()],
        &workspace_root,
    )
    .expect("Transform failed");

    // Windows entries with ONLY problematic features should be removed
    // Linux entries remain, Windows entry with mixed features keeps safe features
    // Expected: 2 Linux entries + 1 Windows entry with default = 3, but actually we get 4
    // because player:asio on Windows gets removed but there might be ordering issues

    // Let's count what we expect:
    // 1. audio_output/asio on Windows -> REMOVED (all features filtered)
    // 2. audio_output/asio on Linux -> KEPT
    // 3. server/full on Windows -> REMOVED (all features filtered)
    // 4. server/full on Linux -> KEPT
    // 5. audio_output/[default,asio] on Windows -> KEPT with just [default]
    // Total: 3 entries

    // But we're getting 4, so let me check...
    // Actually player is not in this test, let me re-read...
    // Yes, we have 5 inputs, expect 3 outputs

    assert_eq!(matrix.len(), 3, "Expected 3 entries after filtering");

    // Check that we have the right entries (order may vary)
    let mut linux_audio = false;
    let mut linux_server = false;
    let mut windows_audio = false;

    for entry in &matrix {
        let pkg = entry.get("package").unwrap().as_str().unwrap();
        let os = entry.get("os").unwrap().as_str().unwrap();

        if pkg == "audio_output" && os == "linux" {
            assert_eq!(entry.get("features").unwrap(), &json!(["asio"]));
            linux_audio = true;
        } else if pkg == "server" && os == "linux" {
            assert_eq!(entry.get("features").unwrap(), &json!(["full"]));
            linux_server = true;
        } else if pkg == "audio_output" && os == "windows" {
            assert_eq!(entry.get("features").unwrap(), &json!(["default"]));
            windows_audio = true;
        }
    }

    assert!(linux_audio, "Expected Linux audio_output with asio");
    assert!(linux_server, "Expected Linux server with full");
    assert!(
        windows_audio,
        "Expected Windows audio_output with default only"
    );
}

#[test]
fn test_platform_filter_multi_features() {
    let workspace_root = fixture_path("basic");
    let script_file = script_path("platform-filter.lua");

    let mut matrix = vec![
        serde_json::Map::from_iter(vec![
            ("os".to_string(), json!("windows")),
            ("features".to_string(), json!(["default", "asio", "jack"])),
        ]),
        serde_json::Map::from_iter(vec![
            ("os".to_string(), json!("linux")),
            ("features".to_string(), json!(["default", "asio"])),
        ]),
    ];

    clippier::transforms::apply_transforms(
        &mut matrix,
        &[script_file.to_string_lossy().to_string()],
        &workspace_root,
    )
    .expect("Transform failed");

    // Windows entry should have asio removed
    assert_eq!(matrix.len(), 2);
    assert_eq!(
        matrix[0].get("features").unwrap(),
        &json!(["default", "jack"])
    );

    // Linux entry should keep all features
    assert_eq!(
        matrix[1].get("features").unwrap(),
        &json!(["default", "asio"])
    );
}

#[test]
fn test_empty_transform_list() {
    let workspace_root = fixture_path("basic");
    let mut matrix = vec![serde_json::Map::from_iter(vec![(
        "value".to_string(),
        json!(1),
    )])];

    clippier::transforms::apply_transforms(&mut matrix, &[], &workspace_root)
        .expect("Transform failed");

    // Matrix should be unchanged
    assert_eq!(matrix.len(), 1);
    assert_eq!(matrix[0].get("value").unwrap(), 1);
}
