//! Integration tests for package filtering functionality.
//!
//! Tests all operators against real Cargo.toml fixtures.

use clippier::package_filter::{matches, parse_filter};
use std::path::PathBuf;

/// Seed the test resources into the simulator if enabled
fn setup() {
    clippier_test_utilities::seed_clippier_test_resources();
}

/// Load a test fixture Cargo.toml file.
fn load_fixture(name: &str) -> toml::Value {
    setup();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("test-resources/package-filter");
    path.push(format!("{name}.toml"));

    let content = switchy_fs::sync::read_to_string(&path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", path.display()));

    toml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse fixture {name}: {e}"))
}

// ============================================================================
// SCALAR OPERATORS: = (Equals)
// ============================================================================

#[switchy_async::test]
async fn test_equals_string_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name=moosicbox_audio_decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_equals_string_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name=different_name").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_equals_boolean_true() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.publish=true").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_equals_boolean_false() {
    let toml = load_fixture("unpublished");
    let filter = parse_filter("package.publish=false").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_equals_version() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.version=0.1.4").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_equals_edition() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.edition=2021").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// SCALAR OPERATORS: != (Not Equals)
// ============================================================================

#[switchy_async::test]
async fn test_not_equals_string() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name!=wrong_name").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_not_equals_boolean() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.publish!=false").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_not_equals_matching_value() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name!=moosicbox_audio_decoder").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

// ============================================================================
// SCALAR OPERATORS: ^= (Starts With)
// ============================================================================

#[switchy_async::test]
async fn test_starts_with_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name^=moosicbox_").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_starts_with_version() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.version^=0.1").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_starts_with_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name^=different_").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_starts_with_full_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name^=moosicbox_audio_decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// SCALAR OPERATORS: $= (Ends With)
// ============================================================================

#[switchy_async::test]
async fn test_ends_with_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name$=_decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_ends_with_example_suffix() {
    let toml = load_fixture("unpublished");
    let filter = parse_filter("package.name$=_example").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_ends_with_package_suffix() {
    let toml = load_fixture("unpublished");
    let filter = parse_filter("package.name$=_package").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_ends_with_full_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name$=moosicbox_audio_decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// SCALAR OPERATORS: *= (Contains)
// ============================================================================

#[switchy_async::test]
async fn test_contains_substring_in_name() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name*=audio").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_contains_substring_in_description() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.description*=decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_contains_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name*=xyz").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_contains_multiple_test_words() {
    let toml = load_fixture("substring-test");
    let filter = parse_filter("package.name*=test").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// SCALAR OPERATORS: ~= (Regex Match)
// ============================================================================

#[switchy_async::test]
async fn test_regex_simple_pattern() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter(r"package.name~=^moosicbox_.*_decoder$").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_regex_version_pattern() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter(r"package.version~=^\d+\.\d+\.\d+$").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_regex_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter(r"package.name~=^test_.*").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_regex_alternative_pattern() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter(r"package.name~=(audio|video)_decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @= (Array Contains Exact)
// ============================================================================

#[switchy_async::test]
async fn test_array_contains_keyword() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@=audio").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_category() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.categories@=multimedia::audio").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_not_contains() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@=nonexistent").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_in_authors() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.authors@=John Doe <john@example.com>").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @*= (Array Contains Substring)
// ============================================================================

#[switchy_async::test]
async fn test_array_contains_substring_in_keywords() {
    let toml = load_fixture("substring-test");
    let filter = parse_filter("package.keywords@*=api").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_substring_partial_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@*=multi").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_substring_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@*=xyz").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @^= (Array Contains Starts With)
// ============================================================================

#[switchy_async::test]
async fn test_array_contains_starts_with_match() {
    let toml = load_fixture("substring-test");
    let filter = parse_filter("package.keywords@^=music").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_starts_with_category() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.categories@^=multimedia").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_starts_with_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@^=xyz").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @~= (Array Contains Regex)
// ============================================================================

#[switchy_async::test]
async fn test_array_contains_regex_pattern() {
    let toml = load_fixture("substring-test");
    let filter = parse_filter(r"package.keywords@~=^music-.*").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_regex_alternative() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter(r"package.keywords@~=(audio|video|multimedia)").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_contains_regex_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter(r"package.keywords@~=^test-.*").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @! (Array Empty)
// ============================================================================

#[switchy_async::test]
async fn test_array_empty_keywords() {
    let toml = load_fixture("empty-arrays");
    let filter = parse_filter("package.keywords@!").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_empty_categories() {
    let toml = load_fixture("empty-arrays");
    let filter = parse_filter("package.categories@!").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_not_empty() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@!").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_empty_missing_property() {
    let toml = load_fixture("no-metadata");
    let filter = parse_filter("package.keywords@!").unwrap();
    // Missing array property should be treated as empty
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @#= (Array Length Equals)
// ============================================================================

#[switchy_async::test]
async fn test_array_length_equals_zero() {
    let toml = load_fixture("empty-arrays");
    let filter = parse_filter("package.keywords@#=0").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_equals_keywords() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.keywords@#=5").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_equals_authors() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.authors@#=3").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_equals_no_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@#=100").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @#> (Array Length Greater)
// ============================================================================

#[switchy_async::test]
async fn test_array_length_greater_than() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.keywords@#>3").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_greater_than_zero() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@#>0").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_not_greater() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.keywords@#>10").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_greater_equal_to_length() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.keywords@#>5").unwrap();
    // Length is exactly 5, not greater
    assert!(!matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: @#< (Array Length Less)
// ============================================================================

#[switchy_async::test]
async fn test_array_length_less_than() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.categories@#<5").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_less_than_large_number() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@#<100").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_not_less() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.keywords@#<3").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_length_less_equal_to_length() {
    let toml = load_fixture("large-arrays");
    let filter = parse_filter("package.keywords@#<5").unwrap();
    // Length is exactly 5, not less
    assert!(!matches(&filter, &toml).unwrap());
}

// ============================================================================
// ARRAY OPERATORS: !@= (Array Not Contains)
// ============================================================================

#[switchy_async::test]
async fn test_array_not_contains_match() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords!@=nonexistent").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_not_contains_existing() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords!@=audio").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_array_not_contains_category() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.categories!@=web-programming").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// EXISTENCE OPERATORS: ? (Exists)
// ============================================================================

#[switchy_async::test]
async fn test_property_exists_name() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_property_exists_readme() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.readme?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_property_exists_missing() {
    let toml = load_fixture("no-metadata");
    let filter = parse_filter("package.homepage?").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_property_exists_documentation() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.documentation?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// EXISTENCE OPERATORS: !? (Not Exists)
// ============================================================================

#[switchy_async::test]
async fn test_property_not_exists_missing() {
    let toml = load_fixture("no-metadata");
    let filter = parse_filter("package.homepage!?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_property_not_exists_readme_missing() {
    let toml = load_fixture("unpublished");
    let filter = parse_filter("package.readme!?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_property_not_exists_present() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name!?").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_property_not_exists_documentation_missing() {
    let toml = load_fixture("unpublished");
    let filter = parse_filter("package.documentation!?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// NESTED PROPERTIES
// ============================================================================

#[switchy_async::test]
async fn test_nested_metadata_workspaces_independent() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.metadata.workspaces.independent=true").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_nested_metadata_custom_field() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.metadata.workspaces.custom-field=test-value").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_nested_metadata_ci_skip_tests() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.metadata.ci.skip-tests=false").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_nested_metadata_exists() {
    let toml = load_fixture("nested-metadata");
    let filter = parse_filter("package.metadata.build.target?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_nested_metadata_not_exists() {
    let toml = load_fixture("no-metadata");
    let filter = parse_filter("package.metadata.workspaces.independent!?").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_deeply_nested_property() {
    let toml = load_fixture("nested-metadata");
    let filter = parse_filter("package.metadata.build.target=wasm32").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_explicit_package_prefix() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name=moosicbox_audio_decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_explicit_package_metadata() {
    let toml = load_fixture("nested-metadata");
    let filter = parse_filter("package.metadata.ci.platform=linux").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// BACKWARD COMPATIBILITY
// ============================================================================

#[switchy_async::test]
async fn test_backward_compat_unprefixed_name() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name=moosicbox_audio_decoder").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_backward_compat_unprefixed_version() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.version^=0.1").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_backward_compat_unprefixed_publish() {
    let toml = load_fixture("unpublished");
    let filter = parse_filter("package.publish=false").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_backward_compat_unprefixed_keywords() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords@=audio").unwrap();
    assert!(matches(&filter, &toml).unwrap());
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[switchy_async::test]
async fn test_edge_case_empty_string_value() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name=").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_edge_case_missing_property_equals() {
    let toml = load_fixture("no-metadata");
    let filter = parse_filter("package.homepage=https://example.com").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_edge_case_invalid_array_length_format() {
    let toml = load_fixture("comprehensive");
    // Parser accepts any string, validation happens during matching
    let filter = parse_filter("package.keywords@#=abc").unwrap();
    let result = matches(&filter, &toml);
    // Should fail during matching because "abc" is not a valid number
    assert!(result.is_err());
}

#[switchy_async::test]
async fn test_edge_case_regex_invalid_pattern() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter(r"package.name~=[invalid").unwrap();
    let result = matches(&filter, &toml);
    assert!(result.is_err());
}

#[switchy_async::test]
async fn test_edge_case_type_mismatch_string_on_boolean() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.publish=yes").unwrap();
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_edge_case_array_operation_on_scalar() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.name@=audio").unwrap();
    // Should not match because name is a string, not an array
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_edge_case_scalar_operation_on_array() {
    let toml = load_fixture("comprehensive");
    let filter = parse_filter("package.keywords=audio").unwrap();
    // Should not match because keywords is an array, not a scalar
    assert!(!matches(&filter, &toml).unwrap());
}

#[switchy_async::test]
async fn test_edge_case_empty_array_length_operations() {
    let toml = load_fixture("empty-arrays");

    // Empty array: length = 0
    let filter_eq = parse_filter("package.keywords@#=0").unwrap();
    assert!(matches(&filter_eq, &toml).unwrap());

    let filter_gt = parse_filter("package.keywords@#>0").unwrap();
    assert!(!matches(&filter_gt, &toml).unwrap());

    let filter_lt = parse_filter("package.keywords@#<1").unwrap();
    assert!(matches(&filter_lt, &toml).unwrap());
}

// ============================================================================
// COMBINED FILTERS (Integration with handler)
// ============================================================================

#[switchy_async::test]
async fn test_combined_skip_and_include_both_match() {
    // Test the logic when both skip-if and include-if would match
    let toml = load_fixture("comprehensive");

    // Include if name starts with moosicbox_
    let include = parse_filter("package.name^=moosicbox_").unwrap();
    assert!(matches(&include, &toml).unwrap());

    // Skip if name ends with _example
    let skip = parse_filter("package.name$=_example").unwrap();
    assert!(!matches(&skip, &toml).unwrap());

    // Logic: include=true AND skip=false -> should be included
}

#[switchy_async::test]
async fn test_combined_skip_and_include_skip_wins() {
    let toml = load_fixture("unpublished");

    // Include if name contains "package"
    let include = parse_filter("package.name*=package").unwrap();
    assert!(matches(&include, &toml).unwrap());

    // Skip if publish = false
    let skip = parse_filter("package.publish=false").unwrap();
    assert!(matches(&skip, &toml).unwrap());

    // Logic: include=true AND skip=true -> should be skipped
}

#[switchy_async::test]
async fn test_combined_multiple_include_conditions() {
    let toml = load_fixture("comprehensive");

    // Multiple conditions that should all match
    let filter1 = parse_filter("package.name^=moosicbox_").unwrap();
    let filter2 = parse_filter("package.keywords@=audio").unwrap();
    let filter3 = parse_filter("package.publish=true").unwrap();

    assert!(matches(&filter1, &toml).unwrap());
    assert!(matches(&filter2, &toml).unwrap());
    assert!(matches(&filter3, &toml).unwrap());
}

#[switchy_async::test]
async fn test_combined_multiple_skip_conditions() {
    let toml = load_fixture("unpublished");

    // Any skip condition matching should exclude
    let filter1 = parse_filter("package.publish=false").unwrap();
    let _filter2 = parse_filter("package.name$=_example").unwrap();

    // At least one matches
    assert!(matches(&filter1, &toml).unwrap());
}
