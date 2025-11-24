#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Custom provider example for `switchy_env`
//!
//! This example demonstrates how to implement a custom `EnvProvider` trait
//! to create alternative environment variable sources. Use cases include:
//! - Loading configuration from files
//! - Combining multiple configuration sources
//! - Adding prefix/namespace support
//! - Implementing custom default values

use std::collections::BTreeMap;
use switchy_env::{EnvError, EnvProvider, Result};

/// A custom environment provider that combines multiple sources
///
/// This provider demonstrates a practical pattern: layered configuration where
/// values can come from different sources with a defined precedence order.
struct LayeredEnvProvider {
    /// Highest priority: explicit overrides
    overrides: BTreeMap<String, String>,
    /// Medium priority: application defaults
    defaults: BTreeMap<String, String>,
}

impl LayeredEnvProvider {
    /// Create a new layered provider with defaults
    fn new() -> Self {
        let mut defaults = BTreeMap::new();

        // Application-specific defaults
        defaults.insert("APP_NAME".to_string(), "MyApp".to_string());
        defaults.insert("APP_VERSION".to_string(), "1.0.0".to_string());
        defaults.insert("LOG_LEVEL".to_string(), "info".to_string());
        defaults.insert("MAX_WORKERS".to_string(), "4".to_string());
        defaults.insert("TIMEOUT_SECONDS".to_string(), "30".to_string());

        Self {
            overrides: BTreeMap::new(),
            defaults,
        }
    }

    /// Add an override value (highest priority)
    fn set_override(&mut self, name: &str, value: &str) {
        self.overrides.insert(name.to_string(), value.to_string());
    }
}

impl EnvProvider for LayeredEnvProvider {
    fn var(&self, name: &str) -> Result<String> {
        // Check overrides first (highest priority)
        if let Some(value) = self.overrides.get(name) {
            return Ok(value.clone());
        }

        // Then check system environment
        if let Ok(value) = std::env::var(name) {
            return Ok(value);
        }

        // Finally check defaults (lowest priority)
        if let Some(value) = self.defaults.get(name) {
            return Ok(value.clone());
        }

        Err(EnvError::NotFound(name.to_string()))
    }

    fn vars(&self) -> BTreeMap<String, String> {
        let mut all_vars = BTreeMap::new();

        // Start with defaults (lowest priority)
        all_vars.extend(self.defaults.clone());

        // Add system environment (medium priority)
        all_vars.extend(std::env::vars());

        // Add overrides (highest priority)
        all_vars.extend(self.overrides.clone());

        all_vars
    }
}

/// A prefixed environment provider that adds a namespace
///
/// This provider demonstrates how to implement namespacing by
/// automatically adding a prefix to all variable names.
struct PrefixedEnvProvider {
    prefix: String,
}

impl PrefixedEnvProvider {
    /// Create a new prefixed provider
    fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    /// Convert a variable name to its prefixed form
    fn prefixed_name(&self, name: &str) -> String {
        format!("{}_{}", self.prefix, name)
    }
}

impl EnvProvider for PrefixedEnvProvider {
    fn var(&self, name: &str) -> Result<String> {
        let prefixed = self.prefixed_name(name);
        std::env::var(&prefixed).map_err(|_| EnvError::NotFound(name.to_string()))
    }

    fn vars(&self) -> BTreeMap<String, String> {
        let prefix_with_underscore = format!("{}_", self.prefix);
        std::env::vars()
            .filter_map(|(key, value)| {
                key.strip_prefix(&prefix_with_underscore)
                    .map(|stripped| (stripped.to_string(), value))
            })
            .collect()
    }
}

fn main() -> Result<()> {
    println!("=== Switchy Env Custom Provider Example ===\n");

    // Example 1: Using a layered provider
    println!("1. Layered Environment Provider:");
    println!("   Combines overrides, system environment, and defaults\n");

    let mut layered = LayeredEnvProvider::new();

    // Access default values
    println!("   Defaults:");
    println!("   - APP_NAME: {}", layered.var("APP_NAME")?);
    println!("   - APP_VERSION: {}", layered.var("APP_VERSION")?);
    println!("   - LOG_LEVEL: {}", layered.var("LOG_LEVEL")?);

    // System environment takes precedence over defaults
    // SAFETY: This is an example program demonstrating environment variable usage
    unsafe {
        std::env::set_var("LOG_LEVEL", "debug");
    }
    println!("\n   After setting LOG_LEVEL=debug in system env:");
    println!("   - LOG_LEVEL: {}", layered.var("LOG_LEVEL")?);

    // Overrides take precedence over everything
    layered.set_override("LOG_LEVEL", "trace");
    println!("\n   After adding override LOG_LEVEL=trace:");
    println!("   - LOG_LEVEL: {}", layered.var("LOG_LEVEL")?);

    // Example 2: Using var_parse with custom provider
    println!("\n2. Type Parsing with Custom Provider:");

    let max_workers: usize = layered.var_parse("MAX_WORKERS")?;
    println!("   MAX_WORKERS (parsed as usize): {max_workers}");

    let timeout: u64 = layered.var_parse("TIMEOUT_SECONDS")?;
    println!("   TIMEOUT_SECONDS (parsed as u64): {timeout}");

    // Example 3: Using a prefixed provider
    println!("\n3. Prefixed Environment Provider:");
    println!("   Automatically adds namespace prefix to variable names\n");

    // Set some prefixed environment variables
    // SAFETY: This is an example program demonstrating environment variable usage
    unsafe {
        std::env::set_var("MYAPP_DATABASE", "postgres://localhost/mydb");
        std::env::set_var("MYAPP_PORT", "3000");
        std::env::set_var("MYAPP_ENABLE_CACHE", "true");
    }

    let prefixed = PrefixedEnvProvider::new("MYAPP");

    // Access variables without the prefix
    println!("   Accessing 'DATABASE' (actually reads 'MYAPP_DATABASE'):");
    println!("   - DATABASE: {}", prefixed.var("DATABASE")?);

    println!("\n   Accessing 'PORT' (actually reads 'MYAPP_PORT'):");
    let port: u16 = prefixed.var_parse("PORT")?;
    println!("   - PORT: {port}");

    println!("\n   Accessing 'ENABLE_CACHE' (actually reads 'MYAPP_ENABLE_CACHE'):");
    let enable_cache: bool = prefixed.var_parse("ENABLE_CACHE")?;
    println!("   - ENABLE_CACHE: {enable_cache}");

    // Example 4: Getting all variables from a custom provider
    println!("\n4. Listing All Variables:");

    println!("\n   All layered provider variables:");
    let layered_vars = layered.vars();
    for (key, value) in layered_vars.iter().take(5) {
        println!("   - {key} = {value}");
    }
    println!("   ... ({} total variables)", layered_vars.len());

    println!("\n   All prefixed provider variables (MYAPP_*):");
    let prefixed_vars = prefixed.vars();
    for (key, value) in &prefixed_vars {
        println!("   - {key} = {value}");
    }

    // Example 5: Using var_or with custom providers
    println!("\n5. Default Values with Custom Providers:");

    let db_pool_size = layered.var_or("DB_POOL_SIZE", "10");
    println!("   DB_POOL_SIZE (with default): {db_pool_size}");

    let cache_ttl: u32 = layered.var_parse_or("CACHE_TTL", 300);
    println!("   CACHE_TTL (with default): {cache_ttl}");

    // Example 6: Error handling with custom providers
    println!("\n6. Error Handling:");

    match layered.var("NONEXISTENT_VAR") {
        Ok(val) => println!("   Found value: {val}"),
        Err(EnvError::NotFound(name)) => println!("   Variable '{name}' not found (expected)"),
        Err(e) => println!("   Unexpected error: {e}"),
    }

    println!("\n=== Example completed successfully! ===");
    Ok(())
}
