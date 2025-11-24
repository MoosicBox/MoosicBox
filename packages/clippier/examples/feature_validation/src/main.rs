#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Feature Validation Example
//!
//! This example demonstrates how to use the clippier library to validate
//! feature propagation across workspace dependencies.

use clippier::{FeatureValidator, OutputType, ValidatorConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging for better visibility
    moosicbox_logging::init(Some("feature_validation_example"), None).ok();

    println!("=== Clippier Feature Validation Example ===\n");

    // Example 1: Validate a specific feature across the workspace
    println!("Example 1: Validating 'fail-on-warnings' feature propagation");
    println!("-----------------------------------------------------------");

    let config = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        workspace_only: true,
        output_format: OutputType::Raw,
    };

    let validator = FeatureValidator::new(None, config)?;
    let result = validator.validate()?;

    println!("Total packages checked: {}", result.total_packages);
    println!("Valid packages: {}", result.valid_packages);
    println!("Packages with errors: {}", result.errors.len());

    if !result.errors.is_empty() {
        println!("\nFound validation errors:");
        for error in &result.errors {
            println!("  - Package: {}", error.package);
        }
    }

    if !result.warnings.is_empty() {
        println!("\nWarnings: {}", result.warnings.len());
    }

    println!();

    // Example 2: Auto-detect features that need validation
    println!("Example 2: Auto-detecting features across workspace");
    println!("----------------------------------------------------");

    let auto_config = ValidatorConfig {
        features: None, // Auto-detect features
        workspace_only: true,
        output_format: OutputType::Raw,
    };

    let auto_validator = FeatureValidator::new(None, auto_config)?;
    let auto_result = auto_validator.validate()?;

    println!(
        "Auto-detected validation for {} packages",
        auto_result.total_packages
    );
    println!("Valid: {}", auto_result.valid_packages);
    println!();

    // Example 3: Validate multiple features
    println!("Example 3: Validating multiple features");
    println!("----------------------------------------");

    let multi_config = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string(), "git-diff".to_string()]),
        workspace_only: true,
        output_format: OutputType::Raw,
    };

    let multi_validator = FeatureValidator::new(None, multi_config)?;
    let multi_result = multi_validator.validate()?;

    println!("Total packages: {}", multi_result.total_packages);
    println!("Valid packages: {}", multi_result.valid_packages);
    println!("Errors: {}", multi_result.errors.len());
    println!();

    // Example 4: JSON output format (useful for CI/CD integration)
    println!("Example 4: JSON output format");
    println!("------------------------------");

    let json_config = ValidatorConfig {
        features: Some(vec!["fail-on-warnings".to_string()]),
        workspace_only: true,
        output_format: OutputType::Json,
    };

    let json_validator = FeatureValidator::new(None, json_config)?;
    let json_result = json_validator.validate()?;

    // Serialize to JSON for inspection
    let json_output = serde_json::to_string_pretty(&json_result)?;
    println!("JSON output (truncated):");
    let lines: Vec<&str> = json_output.lines().take(15).collect();
    for line in lines {
        println!("{line}");
    }
    if json_output.lines().count() > 15 {
        println!("  ... ({} more lines)", json_output.lines().count() - 15);
    }
    println!();

    // Summary
    println!("=== Summary ===");
    println!("Successfully demonstrated feature validation using clippier library");
    println!("Key capabilities shown:");
    println!("  ✓ Single feature validation");
    println!("  ✓ Auto-detection of features");
    println!("  ✓ Multiple feature validation");
    println!("  ✓ JSON output format for CI/CD");

    Ok(())
}
