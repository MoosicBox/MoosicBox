//! Custom provider example for `switchy_env`
//!
//! This example demonstrates how to implement custom environment variable providers:
//! - Creating a basic in-memory provider
//! - Implementing a case-insensitive provider
//! - Building configuration-specific providers
//! - Using the `EnvProvider` trait for custom sources

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;
use switchy_env::{EnvError, EnvProvider, Result};

/// A simple in-memory environment provider with static variables
struct StaticEnv {
    vars: BTreeMap<String, String>,
}

impl StaticEnv {
    /// Create a new static environment with predefined variables
    fn new() -> Self {
        let mut vars = BTreeMap::new();
        vars.insert("APP_NAME".to_string(), "MyApp".to_string());
        vars.insert("APP_VERSION".to_string(), "1.0.0".to_string());
        vars.insert("PORT".to_string(), "8080".to_string());
        vars.insert("DEBUG".to_string(), "true".to_string());

        Self { vars }
    }

    /// Create a custom static environment from a map
    const fn from_map(vars: BTreeMap<String, String>) -> Self {
        Self { vars }
    }
}

impl EnvProvider for StaticEnv {
    fn var(&self, name: &str) -> Result<String> {
        self.vars
            .get(name)
            .cloned()
            .ok_or_else(|| EnvError::NotFound(name.to_string()))
    }

    fn vars(&self) -> BTreeMap<String, String> {
        self.vars.clone()
    }
}

/// An environment provider that transforms variable names to uppercase
struct CaseInsensitiveEnv {
    vars: BTreeMap<String, String>,
}

impl CaseInsensitiveEnv {
    /// Create a new case-insensitive environment provider
    fn new() -> Self {
        let mut vars = BTreeMap::new();
        vars.insert("database_url".to_uppercase(), "sqlite::memory:".to_string());
        vars.insert("max_connections".to_uppercase(), "10".to_string());
        vars.insert("timeout".to_uppercase(), "30".to_string());

        Self { vars }
    }
}

impl EnvProvider for CaseInsensitiveEnv {
    fn var(&self, name: &str) -> Result<String> {
        let upper_name = name.to_uppercase();
        self.vars
            .get(&upper_name)
            .cloned()
            .ok_or_else(|| EnvError::NotFound(name.to_string()))
    }

    fn vars(&self) -> BTreeMap<String, String> {
        self.vars.clone()
    }
}

/// A provider that wraps another provider and adds a prefix to all lookups
struct PrefixedEnv<T: EnvProvider> {
    prefix: String,
    inner: T,
}

impl<T: EnvProvider> PrefixedEnv<T> {
    /// Create a new prefixed environment provider
    fn new(prefix: impl Into<String>, inner: T) -> Self {
        Self {
            prefix: prefix.into(),
            inner,
        }
    }
}

impl<T: EnvProvider> EnvProvider for PrefixedEnv<T> {
    fn var(&self, name: &str) -> Result<String> {
        // Add prefix to the variable name before lookup
        let prefixed = format!("{}_{}", self.prefix, name);
        self.inner.var(&prefixed)
    }

    fn vars(&self) -> BTreeMap<String, String> {
        let prefix_len = self.prefix.len() + 1; // +1 for underscore
        self.inner
            .vars()
            .into_iter()
            .filter_map(|(key, value)| {
                // Remove prefix from keys
                if key.starts_with(&self.prefix) && key.len() > prefix_len {
                    Some((key[prefix_len..].to_string(), value))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// A provider that combines two providers with priority
struct MergedEnv<T1: EnvProvider, T2: EnvProvider> {
    high_priority: T1,
    low_priority: T2,
}

impl<T1: EnvProvider, T2: EnvProvider> MergedEnv<T1, T2> {
    /// Create a new merged environment with two providers
    const fn new(high_priority: T1, low_priority: T2) -> Self {
        Self {
            high_priority,
            low_priority,
        }
    }
}

impl<T1: EnvProvider, T2: EnvProvider> EnvProvider for MergedEnv<T1, T2> {
    fn var(&self, name: &str) -> Result<String> {
        // Try high priority first, then fall back to low priority
        self.high_priority
            .var(name)
            .or_else(|_| self.low_priority.var(name))
    }

    fn vars(&self) -> BTreeMap<String, String> {
        // Start with low priority, then override with high priority
        let mut all_vars = self.low_priority.vars();
        all_vars.extend(self.high_priority.vars());
        all_vars
    }
}

#[allow(clippy::too_many_lines)]
fn main() {
    println!("=== switchy_env Custom Provider Example ===\n");

    // 1. Static environment provider
    println!("1. Static environment provider:");
    let static_env = StaticEnv::new();
    println!("   APP_NAME = {}", static_env.var("APP_NAME").unwrap());
    println!(
        "   APP_VERSION = {}",
        static_env.var("APP_VERSION").unwrap()
    );
    println!("   PORT = {}", static_env.var("PORT").unwrap());

    // Using `EnvProvider` methods
    println!(
        "   DEBUG (parsed) = {}",
        static_env.var_parse::<bool>("DEBUG").unwrap()
    );
    println!(
        "   UNKNOWN (with default) = {}",
        static_env.var_or("UNKNOWN", "default_value")
    );

    // 2. Case-insensitive provider
    println!("\n2. Case-insensitive environment provider:");
    let case_insensitive = CaseInsensitiveEnv::new();

    println!(
        "   database_url = {}",
        case_insensitive.var("database_url").unwrap()
    );
    println!(
        "   DATABASE_URL = {}",
        case_insensitive.var("DATABASE_URL").unwrap()
    );
    println!(
        "   DaTaBaSe_UrL = {}",
        case_insensitive.var("DaTaBaSe_UrL").unwrap()
    );

    // 3. Prefixed environment provider
    println!("\n3. Prefixed environment provider:");

    let mut base_vars = BTreeMap::new();
    base_vars.insert("MYAPP_HOST".to_string(), "localhost".to_string());
    base_vars.insert("MYAPP_PORT".to_string(), "3000".to_string());
    base_vars.insert("OTHER_VAR".to_string(), "ignored".to_string());

    let base_env = StaticEnv::from_map(base_vars);
    let prefixed = PrefixedEnv::new("MYAPP", base_env);

    println!("   HOST = {}", prefixed.var("HOST").unwrap());
    println!("   PORT = {}", prefixed.var("PORT").unwrap());
    println!("   Available vars: {:?}", prefixed.vars().keys());

    // 4. Merged environment provider
    println!("\n4. Merged environment provider:");

    let mut high_priority = BTreeMap::new();
    high_priority.insert("PORT".to_string(), "9000".to_string());
    high_priority.insert("CUSTOM".to_string(), "high_priority_value".to_string());

    let high_priority_env = StaticEnv::from_map(high_priority);
    let low_priority_env = StaticEnv::new();

    let merged = MergedEnv::new(high_priority_env, low_priority_env);

    println!(
        "   PORT = {} (from high priority)",
        merged.var("PORT").unwrap()
    );
    println!(
        "   APP_NAME = {} (from low priority)",
        merged.var("APP_NAME").unwrap()
    );
    println!(
        "   CUSTOM = {} (only in high priority)",
        merged.var("CUSTOM").unwrap()
    );

    // 5. Complex composition
    println!("\n5. Complex configuration with composition:");

    let mut prod_config = BTreeMap::new();
    prod_config.insert("MYAPP_ENV".to_string(), "production".to_string());
    prod_config.insert("MYAPP_PORT".to_string(), "443".to_string());

    let mut defaults = BTreeMap::new();
    defaults.insert("MYAPP_ENV".to_string(), "development".to_string());
    defaults.insert("MYAPP_PORT".to_string(), "8080".to_string());
    defaults.insert("MYAPP_HOST".to_string(), "0.0.0.0".to_string());
    defaults.insert("MYAPP_WORKERS".to_string(), "4".to_string());

    let prod_env = StaticEnv::from_map(prod_config);
    let default_env = StaticEnv::from_map(defaults);

    // Merge them with production taking priority
    let merged = MergedEnv::new(prod_env, default_env);
    // Then add prefix support
    let prefixed = PrefixedEnv::new("MYAPP", merged);

    println!(
        "   ENV = {} (from production)",
        prefixed.var("ENV").unwrap()
    );
    println!(
        "   PORT = {} (from production)",
        prefixed.var("PORT").unwrap()
    );
    println!(
        "   HOST = {} (from defaults)",
        prefixed.var("HOST").unwrap()
    );
    println!(
        "   WORKERS = {} (from defaults)",
        prefixed.var("WORKERS").unwrap()
    );

    // 6. Demonstrate using `var_parse_opt` with custom provider
    println!("\n6. Using `EnvProvider` trait methods:");
    let env = StaticEnv::new();

    match env.var_parse_opt::<u16>("PORT") {
        Ok(Some(port)) => println!("   PORT exists: {port}"),
        Ok(None) => println!("   PORT not set"),
        Err(e) => println!("   Error: {e}"),
    }

    match env.var_parse_opt::<u16>("MISSING") {
        Ok(Some(port)) => println!("   MISSING exists: {port}"),
        Ok(None) => println!("   MISSING not set (as expected)"),
        Err(e) => println!("   Error: {e}"),
    }

    println!("\n=== Example Complete ===");
    println!("\nKey takeaway: Implement `EnvProvider` to create custom environment");
    println!("variable sources for advanced configuration management.");
}
