//! Simulator testing example for `switchy_env`
//!
//! This example demonstrates how to use the simulator mode for deterministic testing:
//! - Using predefined simulator defaults
//! - Setting custom variables for test scenarios
//! - Resetting and clearing the environment
//! - Testing environment-dependent code

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_env::simulator::{clear, remove_var, reset, set_var};
use switchy_env::{var, var_exists, var_parse};

/// Example configuration struct that depends on environment variables
#[derive(Debug)]
#[allow(dead_code)]
struct AppConfig {
    port: u16,
    database_url: String,
    debug: bool,
    max_connections: usize,
}

impl AppConfig {
    /// Load configuration from environment variables
    fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            port: var_parse("PORT")?,
            database_url: var("DATABASE_URL")?,
            debug: var_parse("DEBUG_RENDERER")?,
            max_connections: var_parse("SIMULATOR_MAX_PARALLEL")?,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== switchy_env Simulator Testing Example ===\n");

    // 1. Show default simulator values
    println!("1. Default simulator environment:");
    println!("   DATABASE_URL = {}", var("DATABASE_URL")?);
    println!("   PORT = {}", var("PORT")?);
    println!("   SIMULATOR_SEED = {}", var("SIMULATOR_SEED")?);
    println!("   DEBUG_RENDERER = {}", var("DEBUG_RENDERER")?);

    // 2. Load configuration with defaults
    println!("\n2. Loading app configuration with defaults:");
    let config = AppConfig::from_env()?;
    println!("   {config:?}");

    // 3. Set custom test values
    println!("\n3. Setting custom test values:");
    set_var("PORT", "9000");
    set_var("DATABASE_URL", "postgresql://test:test@localhost/testdb");
    set_var("DEBUG_RENDERER", "1");
    set_var("CUSTOM_VAR", "custom_value");

    println!("   PORT = {}", var("PORT")?);
    println!("   DATABASE_URL = {}", var("DATABASE_URL")?);
    println!("   DEBUG_RENDERER = {}", var("DEBUG_RENDERER")?);
    println!("   CUSTOM_VAR = {}", var("CUSTOM_VAR")?);

    // 4. Test with modified environment
    println!("\n4. Loading config with custom values:");
    let config = AppConfig::from_env()?;
    println!("   {config:?}");

    // 5. Remove a variable
    println!("\n5. Removing CUSTOM_VAR:");
    remove_var("CUSTOM_VAR");
    println!("   CUSTOM_VAR exists: {}", var_exists("CUSTOM_VAR"));

    // 6. Reset to defaults
    println!("\n6. Resetting to defaults:");
    reset();
    println!("   PORT = {}", var("PORT")?);
    println!("   DATABASE_URL = {}", var("DATABASE_URL")?);
    println!("   CUSTOM_VAR exists: {}", var_exists("CUSTOM_VAR"));

    // 7. Clear all variables
    println!("\n7. Clearing all variables:");
    clear();
    println!("   PORT exists: {}", var_exists("PORT"));
    println!("   DATABASE_URL exists: {}", var_exists("DATABASE_URL"));

    // Attempting to load config now should fail
    match AppConfig::from_env() {
        Ok(_) => println!("   Unexpected success loading config"),
        Err(e) => println!("   Expected error: {e}"),
    }

    // 8. Demonstrate test scenario setup
    println!("\n8. Setting up a test scenario:");
    reset(); // Start fresh

    // Simulate a production-like environment
    set_var("PORT", "443");
    set_var("DATABASE_URL", "postgresql://prod:pass@db.example.com/prod");
    set_var("DEBUG_RENDERER", "0");
    set_var("SIMULATOR_MAX_PARALLEL", "10");

    let prod_config = AppConfig::from_env()?;
    println!("   Production-like config: {prod_config:?}");

    // 9. Demonstrate testing different scenarios
    println!("\n9. Testing multiple scenarios:");

    // Scenario A: Development mode
    reset();
    set_var("PORT", "3000");
    set_var("DEBUG_RENDERER", "1");
    set_var("SIMULATOR_MAX_PARALLEL", "1");
    let dev_config = AppConfig::from_env()?;
    println!("   Development config: {dev_config:?}");

    // Scenario B: Testing mode
    reset();
    set_var("PORT", "8080");
    set_var("DATABASE_URL", "sqlite::memory:");
    set_var("DEBUG_RENDERER", "1");
    let test_config = AppConfig::from_env()?;
    println!("   Test config: {test_config:?}");

    println!("\n=== Example Complete ===");
    println!("\nKey takeaway: The simulator allows you to test your code with");
    println!("different environment configurations in a controlled, reproducible way.");

    Ok(())
}
