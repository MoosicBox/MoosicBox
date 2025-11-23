//! Simulator environment for testing.
//!
//! This module provides a configurable environment with deterministic defaults
//! for testing. It maintains its own set of environment variables separate from
//! the system environment, allowing for controlled and reproducible tests.
//!
//! The simulator automatically initializes with real environment variables and
//! adds simulator-specific defaults for common configuration values.
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "simulator")]
//! # {
//! use switchy_env::simulator::{set_var, var, reset};
//!
//! // Set a test variable
//! set_var("DATABASE_URL", "sqlite::memory:");
//!
//! // Access it like normal
//! let db_url = var("DATABASE_URL").unwrap();
//! assert_eq!(db_url, "sqlite::memory:");
//!
//! // Reset to defaults
//! reset();
//! # }
//! ```

use crate::{EnvError, EnvProvider, Result};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Simulator environment provider with configurable variables
pub struct SimulatorEnv {
    vars: Arc<RwLock<BTreeMap<String, String>>>,
}

impl SimulatorEnv {
    /// Creates a new simulator environment provider with default values
    ///
    /// Initializes the environment with real environment variables and adds
    /// simulator-specific defaults for testing and deterministic behavior.
    #[must_use]
    pub fn new() -> Self {
        let mut vars = BTreeMap::new();

        // Load real environment variables as defaults
        for (key, value) in std::env::vars() {
            vars.insert(key, value);
        }

        // Override with simulator-specific defaults
        Self::set_simulator_defaults(&mut vars);

        Self {
            vars: Arc::new(RwLock::new(vars)),
        }
    }

    /// Set a variable for testing
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    pub fn set_var(&self, name: &str, value: &str) {
        let mut vars = self.vars.write().unwrap();
        vars.insert(name.to_string(), value.to_string());
    }

    /// Remove a variable
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    pub fn remove_var(&self, name: &str) {
        let mut vars = self.vars.write().unwrap();
        vars.remove(name);
    }

    /// Clear all variables
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    pub fn clear(&self) {
        let mut vars = self.vars.write().unwrap();
        vars.clear();
    }

    /// Reset to defaults
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    pub fn reset(&self) {
        let mut vars = self.vars.write().unwrap();
        vars.clear();

        // Reload real environment variables
        for (key, value) in std::env::vars() {
            vars.insert(key, value);
        }
        drop(vars);

        // Reacquire lock for setting defaults
        let mut vars = self.vars.write().unwrap();
        Self::set_simulator_defaults(&mut vars);
    }

    fn set_simulator_defaults(vars: &mut BTreeMap<String, String>) {
        // Set deterministic defaults for common variables
        vars.entry("SIMULATOR_SEED".to_string())
            .or_insert_with(|| "12345".to_string());
        vars.entry("SIMULATOR_UUID_SEED".to_string())
            .or_insert_with(|| "54321".to_string());
        vars.entry("SIMULATOR_EPOCH_OFFSET".to_string())
            .or_insert_with(|| "0".to_string());
        vars.entry("SIMULATOR_STEP_MULTIPLIER".to_string())
            .or_insert_with(|| "1".to_string());
        vars.entry("SIMULATOR_RUNS".to_string())
            .or_insert_with(|| "1".to_string());
        vars.entry("SIMULATOR_MAX_PARALLEL".to_string())
            .or_insert_with(|| "1".to_string());
        vars.entry("SIMULATOR_DURATION".to_string())
            .or_insert_with(|| "60".to_string());

        // Database defaults for testing
        vars.entry("DATABASE_URL".to_string())
            .or_insert_with(|| "sqlite::memory:".to_string());
        vars.entry("DB_HOST".to_string())
            .or_insert_with(|| "localhost".to_string());
        vars.entry("DB_NAME".to_string())
            .or_insert_with(|| "test_db".to_string());
        vars.entry("DB_USER".to_string())
            .or_insert_with(|| "test_user".to_string());
        vars.entry("DB_PASSWORD".to_string())
            .or_insert_with(|| "test_password".to_string());

        // Service defaults
        vars.entry("PORT".to_string())
            .or_insert_with(|| "8080".to_string());
        vars.entry("SSL_PORT".to_string())
            .or_insert_with(|| "8443".to_string());

        // Debug defaults
        vars.entry("DEBUG_RENDERER".to_string())
            .or_insert_with(|| "0".to_string());
        vars.entry("TOKIO_CONSOLE".to_string())
            .or_insert_with(|| "0".to_string());

        log::debug!(
            "Set simulator environment defaults: {} variables",
            vars.len()
        );
    }
}

impl Default for SimulatorEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvProvider for SimulatorEnv {
    /// Get an environment variable as a string
    ///
    /// # Errors
    ///
    /// * If the environment variable is not found
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    fn var(&self, name: &str) -> Result<String> {
        let vars = self.vars.read().unwrap();
        vars.get(name)
            .cloned()
            .ok_or_else(|| EnvError::NotFound(name.to_string()))
    }

    /// Get all environment variables
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    fn vars(&self) -> BTreeMap<String, String> {
        let vars = self.vars.read().unwrap();
        vars.clone()
    }
}

static PROVIDER: std::sync::LazyLock<SimulatorEnv> = std::sync::LazyLock::new(SimulatorEnv::new);

/// Get an environment variable as a string
///
/// # Errors
///
/// * If the environment variable is not found
///
/// # Panics
///
/// * If the internal `RwLock` is poisoned
pub fn var(name: &str) -> Result<String> {
    PROVIDER.var(name)
}

/// Get an environment variable with a default value
pub fn var_or(name: &str, default: &str) -> String {
    PROVIDER.var_or(name, default)
}

/// Get an environment variable parsed as a specific type
///
/// # Errors
///
/// * If the environment variable is not found
/// * If the environment variable value cannot be parsed to the target type
pub fn var_parse<T>(name: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    PROVIDER.var_parse(name)
}

/// Get an environment variable parsed with a default value
pub fn var_parse_or<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    PROVIDER.var_parse_or(name, default)
}

/// Get an optional environment variable parsed as a specific type
///
/// # Returns
///
/// * `Ok(Some(value))` if the variable exists and parses successfully
/// * `Ok(None)` if the variable doesn't exist
/// * `Err(EnvError::ParseError)` if the variable exists but can't be parsed
///
/// # Errors
///
/// * If the environment variable exists but cannot be parsed to the target type
pub fn var_parse_opt<T>(name: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    PROVIDER.var_parse_opt(name)
}

/// Check if an environment variable exists
pub fn var_exists(name: &str) -> bool {
    PROVIDER.var_exists(name)
}

/// Get all environment variables
///
/// # Panics
///
/// * If the internal `RwLock` is poisoned
pub fn vars() -> BTreeMap<String, String> {
    PROVIDER.vars()
}

/// Set a variable for testing (simulator only)
///
/// # Panics
///
/// * If the internal `RwLock` is poisoned
pub fn set_var(name: &str, value: &str) {
    PROVIDER.set_var(name, value);
}

/// Remove a variable (simulator only)
///
/// # Panics
///
/// * If the internal `RwLock` is poisoned
pub fn remove_var(name: &str) {
    PROVIDER.remove_var(name);
}

/// Clear all variables (simulator only)
///
/// # Panics
///
/// * If the internal `RwLock` is poisoned
pub fn clear() {
    PROVIDER.clear();
}

/// Reset to defaults (simulator only)
///
/// # Panics
///
/// * If the internal `RwLock` is poisoned
pub fn reset() {
    PROVIDER.reset();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EnvProvider;

    #[test_log::test]
    fn test_simulator_env_new_has_defaults() {
        let env = SimulatorEnv::new();
        assert_eq!(env.var("SIMULATOR_SEED").unwrap(), "12345");
        assert_eq!(env.var("SIMULATOR_UUID_SEED").unwrap(), "54321");
        assert_eq!(env.var("PORT").unwrap(), "8080");
        assert_eq!(env.var("DATABASE_URL").unwrap(), "sqlite::memory:");
    }

    #[test_log::test]
    fn test_simulator_env_set_and_get_var() {
        let env = SimulatorEnv::new();
        env.set_var("TEST_VAR", "test_value");
        assert_eq!(env.var("TEST_VAR").unwrap(), "test_value");
    }

    #[test_log::test]
    fn test_simulator_env_set_var_overwrites() {
        let env = SimulatorEnv::new();
        env.set_var("TEST_VAR", "first");
        env.set_var("TEST_VAR", "second");
        assert_eq!(env.var("TEST_VAR").unwrap(), "second");
    }

    #[test_log::test]
    fn test_simulator_env_remove_var() {
        let env = SimulatorEnv::new();
        env.set_var("TEST_VAR", "test_value");
        assert_eq!(env.var("TEST_VAR").unwrap(), "test_value");

        env.remove_var("TEST_VAR");
        assert!(matches!(
            env.var("TEST_VAR"),
            Err(EnvError::NotFound(ref name)) if name == "TEST_VAR"
        ));
    }

    #[test_log::test]
    fn test_simulator_env_clear() {
        let env = SimulatorEnv::new();
        env.set_var("TEST_VAR", "test_value");

        env.clear();

        // After clear, even defaults should be gone
        assert!(matches!(
            env.var("SIMULATOR_SEED"),
            Err(EnvError::NotFound(_))
        ));
        assert!(matches!(env.var("TEST_VAR"), Err(EnvError::NotFound(_))));
    }

    #[test_log::test]
    fn test_simulator_env_reset() {
        let env = SimulatorEnv::new();
        env.set_var("CUSTOM_VAR", "custom_value");
        env.set_var("PORT", "9999");

        env.reset();

        // Custom variable should be gone
        assert!(matches!(env.var("CUSTOM_VAR"), Err(EnvError::NotFound(_))));

        // Default should be restored
        assert_eq!(env.var("PORT").unwrap(), "8080");
    }

    #[test_log::test]
    fn test_simulator_env_var_or_with_existing() {
        let env = SimulatorEnv::new();
        env.set_var("TEST_VAR", "actual_value");
        assert_eq!(env.var_or("TEST_VAR", "default"), "actual_value");
    }

    #[test_log::test]
    fn test_simulator_env_var_or_with_missing() {
        let env = SimulatorEnv::new();
        assert_eq!(env.var_or("MISSING_VAR", "default_value"), "default_value");
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_success() {
        let env = SimulatorEnv::new();
        env.set_var("NUMBER", "42");
        let result: i32 = env.var_parse("NUMBER").unwrap();
        assert_eq!(result, 42);
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_error() {
        let env = SimulatorEnv::new();
        env.set_var("NOT_A_NUMBER", "abc");
        let result: Result<i32> = env.var_parse("NOT_A_NUMBER");
        assert!(matches!(result, Err(EnvError::ParseError(_, _))));
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_not_found() {
        let env = SimulatorEnv::new();
        let result: Result<i32> = env.var_parse("MISSING");
        assert!(matches!(result, Err(EnvError::NotFound(_))));
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_or_with_valid() {
        let env = SimulatorEnv::new();
        env.set_var("NUMBER", "100");
        let result: i32 = env.var_parse_or("NUMBER", 42);
        assert_eq!(result, 100);
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_or_with_invalid() {
        let env = SimulatorEnv::new();
        env.set_var("NOT_A_NUMBER", "xyz");
        let result: i32 = env.var_parse_or("NOT_A_NUMBER", 42);
        assert_eq!(result, 42);
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_or_with_missing() {
        let env = SimulatorEnv::new();
        let result: i32 = env.var_parse_or("MISSING", 42);
        assert_eq!(result, 42);
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_opt_some() {
        let env = SimulatorEnv::new();
        env.set_var("NUMBER", "123");
        let result: Option<i32> = env.var_parse_opt("NUMBER").unwrap();
        assert_eq!(result, Some(123));
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_opt_none() {
        let env = SimulatorEnv::new();
        let result: Option<i32> = env.var_parse_opt("MISSING").unwrap();
        assert_eq!(result, None);
    }

    #[test_log::test]
    fn test_simulator_env_var_parse_opt_parse_error() {
        let env = SimulatorEnv::new();
        env.set_var("NOT_A_NUMBER", "not_an_int");
        let result: Result<Option<i32>> = env.var_parse_opt("NOT_A_NUMBER");
        assert!(matches!(result, Err(EnvError::ParseError(_, _))));
    }

    #[test_log::test]
    fn test_simulator_env_var_exists_true() {
        let env = SimulatorEnv::new();
        env.set_var("EXISTS", "yes");
        assert!(env.var_exists("EXISTS"));
    }

    #[test_log::test]
    fn test_simulator_env_var_exists_false() {
        let env = SimulatorEnv::new();
        assert!(!env.var_exists("DOES_NOT_EXIST"));
    }

    #[test_log::test]
    fn test_simulator_env_vars() {
        let env = SimulatorEnv::new();
        env.clear();
        env.set_var("VAR1", "value1");
        env.set_var("VAR2", "value2");

        let vars = env.vars();
        assert_eq!(vars.get("VAR1").map(String::as_str), Some("value1"));
        assert_eq!(vars.get("VAR2").map(String::as_str), Some("value2"));
        assert_eq!(vars.len(), 2);
    }

    #[test_log::test]
    fn test_simulator_env_default_trait() {
        let env1 = SimulatorEnv::default();
        let env2 = SimulatorEnv::new();

        // Both should have the same defaults
        assert_eq!(env1.var("PORT").unwrap(), env2.var("PORT").unwrap());
    }

    #[test_log::test]
    fn test_global_var() {
        // This tests the global PROVIDER functions
        // Ensure the variable doesn't exist from a previous test
        remove_var("GLOBAL_TEST");
        set_var("GLOBAL_TEST", "global_value");
        assert_eq!(var("GLOBAL_TEST").unwrap(), "global_value");
        remove_var("GLOBAL_TEST");
    }

    #[test_log::test]
    fn test_global_var_or() {
        remove_var("MISSING_GLOBAL");
        assert_eq!(var_or("MISSING_GLOBAL", "fallback"), "fallback");
    }

    #[test_log::test]
    fn test_global_var_parse() {
        set_var("GLOBAL_NUMBER", "777");
        let result: i32 = var_parse("GLOBAL_NUMBER").unwrap();
        assert_eq!(result, 777);
        remove_var("GLOBAL_NUMBER");
    }

    #[test_log::test]
    fn test_global_var_parse_or() {
        remove_var("MISSING_NUMBER");
        let result: i32 = var_parse_or("MISSING_NUMBER", 999);
        assert_eq!(result, 999);
    }

    #[test_log::test]
    fn test_global_var_parse_opt() {
        set_var("OPTIONAL_NUMBER", "555");
        let result: Option<i32> = var_parse_opt("OPTIONAL_NUMBER").unwrap();
        assert_eq!(result, Some(555));
        remove_var("OPTIONAL_NUMBER");
    }

    #[test_log::test]
    fn test_global_var_exists() {
        // Ensure clean state
        remove_var("EXISTS_GLOBAL");
        assert!(!var_exists("EXISTS_GLOBAL"));

        set_var("EXISTS_GLOBAL", "yes");
        assert!(var_exists("EXISTS_GLOBAL"));
        remove_var("EXISTS_GLOBAL");
        assert!(!var_exists("EXISTS_GLOBAL"));
    }

    #[test_log::test]
    fn test_global_vars() {
        clear();
        set_var("VARS_TEST1", "val1");
        set_var("VARS_TEST2", "val2");

        let all_vars = vars();
        assert!(all_vars.contains_key("VARS_TEST1"));
        assert!(all_vars.contains_key("VARS_TEST2"));

        reset();
    }

    #[test_log::test]
    fn test_global_clear() {
        set_var("TO_BE_CLEARED", "value");
        clear();
        assert!(!var_exists("TO_BE_CLEARED"));
        reset(); // Restore defaults for other tests
    }

    #[test_log::test]
    fn test_global_reset() {
        set_var("TO_BE_RESET", "custom");
        reset();
        assert!(!var_exists("TO_BE_RESET"));
        // Defaults should be restored
        assert!(var_exists("PORT"));
    }

    #[test_log::test]
    fn test_simulator_defaults_completeness() {
        let env = SimulatorEnv::new();

        // Test all documented defaults exist
        assert!(env.var_exists("SIMULATOR_SEED"));
        assert!(env.var_exists("SIMULATOR_UUID_SEED"));
        assert!(env.var_exists("SIMULATOR_EPOCH_OFFSET"));
        assert!(env.var_exists("SIMULATOR_STEP_MULTIPLIER"));
        assert!(env.var_exists("SIMULATOR_RUNS"));
        assert!(env.var_exists("SIMULATOR_MAX_PARALLEL"));
        assert!(env.var_exists("SIMULATOR_DURATION"));
        assert!(env.var_exists("DATABASE_URL"));
        assert!(env.var_exists("DB_HOST"));
        assert!(env.var_exists("DB_NAME"));
        assert!(env.var_exists("DB_USER"));
        assert!(env.var_exists("DB_PASSWORD"));
        assert!(env.var_exists("PORT"));
        assert!(env.var_exists("SSL_PORT"));
        assert!(env.var_exists("DEBUG_RENDERER"));
        assert!(env.var_exists("TOKIO_CONSOLE"));
    }

    #[test_log::test]
    fn test_parse_various_types() {
        let env = SimulatorEnv::new();

        // Test bool
        env.set_var("BOOL_TRUE", "true");
        env.set_var("BOOL_FALSE", "false");
        assert!(env.var_parse::<bool>("BOOL_TRUE").unwrap());
        assert!(!env.var_parse::<bool>("BOOL_FALSE").unwrap());

        // Test float
        env.set_var("FLOAT", "2.5");
        assert!((env.var_parse::<f64>("FLOAT").unwrap() - 2.5).abs() < 0.001);

        // Test unsigned
        env.set_var("UNSIGNED", "42");
        assert_eq!(env.var_parse::<u32>("UNSIGNED").unwrap(), 42);
    }
}
