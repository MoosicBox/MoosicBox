#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Simulator testing example for `switchy_env`
//!
//! This example demonstrates using `switchy_env` in simulator mode for testing.
//! The simulator mode provides:
//! - Deterministic defaults for testing
//! - Ability to set/remove variables without affecting system environment
//! - Reset functionality for test isolation

#[cfg(feature = "simulator")]
use switchy_env::simulator::{clear, remove_var, reset, set_var};
use switchy_env::{var, var_exists, var_parse};

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Switchy Env Simulator Testing Example ===\n");

    // Example 1: Accessing simulator defaults
    println!("1. Accessing simulator defaults:");
    println!("   These are deterministic values set by the simulator for testing\n");

    // Simulator seed values
    let simulator_seed = var("SIMULATOR_SEED")?;
    println!("   SIMULATOR_SEED: {simulator_seed}");

    let uuid_seed = var("SIMULATOR_UUID_SEED")?;
    println!("   SIMULATOR_UUID_SEED: {uuid_seed}");

    let epoch_offset = var("SIMULATOR_EPOCH_OFFSET")?;
    println!("   SIMULATOR_EPOCH_OFFSET: {epoch_offset}");

    // Database defaults
    let db_url = var("DATABASE_URL")?;
    println!("   DATABASE_URL: {db_url}");

    let db_host = var("DB_HOST")?;
    println!("   DB_HOST: {db_host}");

    // Service defaults
    let port: u16 = var_parse("PORT")?;
    println!("   PORT: {port}");

    let ssl_port: u16 = var_parse("SSL_PORT")?;
    println!("   SSL_PORT: {ssl_port}");

    // Example 2: Setting custom variables for testing
    println!("\n2. Setting custom variables for testing:");
    #[cfg(feature = "simulator")]
    {
        set_var("TEST_API_KEY", "test_key_12345");
        set_var("TEST_ENDPOINT", "http://localhost:8080/api");

        let api_key = var("TEST_API_KEY")?;
        println!("   TEST_API_KEY: {api_key}");

        let endpoint = var("TEST_ENDPOINT")?;
        println!("   TEST_ENDPOINT: {endpoint}");
    }
    #[cfg(not(feature = "simulator"))]
    {
        println!("   (Simulator mode not enabled - use --features simulator)");
    }

    // Example 3: Removing variables
    println!("\n3. Removing variables:");
    #[cfg(feature = "simulator")]
    {
        println!(
            "   Before removal: TEST_API_KEY exists = {}",
            var_exists("TEST_API_KEY")
        );

        remove_var("TEST_API_KEY");

        println!(
            "   After removal: TEST_API_KEY exists = {}",
            var_exists("TEST_API_KEY")
        );
    }
    #[cfg(not(feature = "simulator"))]
    {
        println!("   (Simulator mode not enabled - use --features simulator)");
    }

    // Example 4: Demonstrating test isolation with reset
    println!("\n4. Demonstrating test isolation:");
    #[cfg(feature = "simulator")]
    {
        // Simulate running a test that modifies the environment
        println!("   Setting up test environment...");
        set_var("TEST_MODE", "integration");
        set_var("TEST_USER", "test@example.com");
        set_var("PORT", "9999"); // Override default

        let test_mode = var("TEST_MODE")?;
        let port_override: u16 = var_parse("PORT")?;
        println!("   TEST_MODE: {test_mode}");
        println!("   PORT (overridden): {port_override}");

        // Reset to defaults for next test
        println!("\n   Resetting environment to defaults...");
        reset();

        println!("   After reset:");
        println!("   TEST_MODE exists: {}", var_exists("TEST_MODE"));
        let port_default: u16 = var_parse("PORT")?;
        println!("   PORT (back to default): {port_default}");
    }
    #[cfg(not(feature = "simulator"))]
    {
        println!("   (Simulator mode not enabled - use --features simulator)");
    }

    // Example 5: Clearing all variables
    println!("\n5. Clearing all variables:");
    #[cfg(feature = "simulator")]
    {
        println!("   Variables before clear:");
        println!("   - DATABASE_URL exists: {}", var_exists("DATABASE_URL"));
        println!("   - PORT exists: {}", var_exists("PORT"));

        clear();

        println!("\n   Variables after clear:");
        println!("   - DATABASE_URL exists: {}", var_exists("DATABASE_URL"));
        println!("   - PORT exists: {}", var_exists("PORT"));

        // Restore for demonstration
        reset();
        println!("\n   After reset, variables are restored:");
        println!("   - DATABASE_URL exists: {}", var_exists("DATABASE_URL"));
    }
    #[cfg(not(feature = "simulator"))]
    {
        println!("   (Simulator mode not enabled - use --features simulator)");
    }

    // Example 6: Practical testing pattern
    println!("\n6. Practical testing pattern:");
    #[cfg(feature = "simulator")]
    {
        println!("   This demonstrates how you might structure a test:\n");

        // Setup
        reset(); // Ensure clean state
        set_var("DB_CONNECTION", "test_connection");
        set_var("CACHE_ENABLED", "true");

        // Test
        let db_conn = var("DB_CONNECTION")?;
        let cache_enabled: bool = var_parse("CACHE_ENABLED")?;

        println!("   Test values:");
        println!("   - DB_CONNECTION: {db_conn}");
        println!("   - CACHE_ENABLED: {cache_enabled}");

        // Teardown
        reset();
        println!("\n   Environment reset for next test");
    }
    #[cfg(not(feature = "simulator"))]
    {
        println!("   (Simulator mode not enabled - use --features simulator)");
    }

    println!("\n=== Example completed successfully! ===");
    println!("\nNote: To see simulator-only features, run with:");
    println!(
        "cargo run --manifest-path packages/env/examples/simulator_testing/Cargo.toml --features simulator"
    );

    Ok(())
}
