//! Standard system environment variable access.
//!
//! This module provides access to real system environment variables via `std::env`.
//! It implements the [`EnvProvider`](crate::EnvProvider) trait to offer a unified
//! interface for environment variable operations.
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "std")]
//! # {
//! use switchy_env::standard::{var, var_parse};
//!
//! # unsafe { std::env::set_var("PORT", "8080"); }
//! // Get a variable as a string
//! let port_str = var("PORT").unwrap();
//!
//! // Parse a variable as a specific type
//! let port: u16 = var_parse("PORT").unwrap();
//! # }
//! ```

use crate::{EnvError, EnvProvider, Result};
use std::collections::BTreeMap;

/// Standard environment provider that uses `std::env`
pub struct StandardEnv;

impl StandardEnv {
    /// Creates a new standard environment provider
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for StandardEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvProvider for StandardEnv {
    /// Get an environment variable as a string
    ///
    /// # Errors
    ///
    /// * If the environment variable is not found
    fn var(&self, name: &str) -> Result<String> {
        std::env::var(name).map_err(|_| EnvError::NotFound(name.to_string()))
    }

    /// Get all environment variables
    fn vars(&self) -> BTreeMap<String, String> {
        std::env::vars().collect()
    }
}

static PROVIDER: std::sync::LazyLock<StandardEnv> = std::sync::LazyLock::new(StandardEnv::new);

/// Get an environment variable as a string
///
/// # Errors
///
/// * If the environment variable is not found
pub fn var(name: &str) -> Result<String> {
    PROVIDER.var(name)
}

/// Get an environment variable with a default value
#[must_use]
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
#[must_use]
pub fn var_exists(name: &str) -> bool {
    PROVIDER.var_exists(name)
}

/// Get all environment variables
#[must_use]
pub fn vars() -> BTreeMap<String, String> {
    PROVIDER.vars()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EnvProvider;
    use serial_test::serial;

    #[test_log::test]
    fn test_standard_env_new() {
        let _env = StandardEnv::new();
        // Just verify we can create it without panicking
    }

    // Tests that modify environment variables must run serially to avoid interference
    // between parallel test executions, since env vars are process-global.
    #[test_log::test]
    #[serial]
    fn test_standard_env_default_trait() {
        let env1 = StandardEnv;
        let env2 = StandardEnv::new();

        // Both should access the same system environment
        assert_eq!(env1.vars().len(), env2.vars().len());
    }

    #[test_log::test]
    fn test_standard_env_var_not_found() {
        let env = StandardEnv::new();
        let result = env.var("VERY_UNLIKELY_TO_EXIST_VAR_123456789");
        assert!(matches!(result, Err(EnvError::NotFound(_))));
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_exists_with_set_var() {
        // Set a test variable in the actual environment
        unsafe {
            std::env::set_var("STANDARD_ENV_TEST_VAR", "test_value");
        }

        let env = StandardEnv::new();
        assert_eq!(env.var("STANDARD_ENV_TEST_VAR").unwrap(), "test_value");

        // Cleanup
        unsafe {
            std::env::remove_var("STANDARD_ENV_TEST_VAR");
        }
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_or_with_existing() {
        unsafe {
            std::env::set_var("STANDARD_ENV_TEST_VAR_OR", "actual");
        }

        let env = StandardEnv::new();
        assert_eq!(env.var_or("STANDARD_ENV_TEST_VAR_OR", "default"), "actual");

        unsafe {
            std::env::remove_var("STANDARD_ENV_TEST_VAR_OR");
        }
    }

    #[test_log::test]
    fn test_standard_env_var_or_with_missing() {
        let env = StandardEnv::new();
        assert_eq!(
            env.var_or("DEFINITELY_MISSING_VAR_123456", "default_value"),
            "default_value"
        );
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_parse() {
        unsafe {
            std::env::set_var("STANDARD_ENV_NUMBER", "42");
        }

        let env = StandardEnv::new();
        let result: i32 = env.var_parse("STANDARD_ENV_NUMBER").unwrap();
        assert_eq!(result, 42);

        unsafe {
            std::env::remove_var("STANDARD_ENV_NUMBER");
        }
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_parse_error() {
        unsafe {
            std::env::set_var("STANDARD_ENV_NOT_A_NUMBER", "not_a_number");
        }

        let env = StandardEnv::new();
        let result: Result<i32> = env.var_parse("STANDARD_ENV_NOT_A_NUMBER");
        assert!(matches!(result, Err(EnvError::ParseError(_, _))));

        unsafe {
            std::env::remove_var("STANDARD_ENV_NOT_A_NUMBER");
        }
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_parse_or() {
        unsafe {
            std::env::set_var("STANDARD_ENV_PARSE_OR", "100");
        }

        let env = StandardEnv::new();
        let result: i32 = env.var_parse_or("STANDARD_ENV_PARSE_OR", 42);
        assert_eq!(result, 100);

        unsafe {
            std::env::remove_var("STANDARD_ENV_PARSE_OR");
        }
    }

    #[test_log::test]
    fn test_standard_env_var_parse_or_missing() {
        let env = StandardEnv::new();
        let result: i32 = env.var_parse_or("MISSING_VAR_999", 42);
        assert_eq!(result, 42);
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_parse_opt_some() {
        unsafe {
            std::env::set_var("STANDARD_ENV_OPT", "123");
        }

        let env = StandardEnv::new();
        let result: Option<i32> = env.var_parse_opt("STANDARD_ENV_OPT").unwrap();
        assert_eq!(result, Some(123));

        unsafe {
            std::env::remove_var("STANDARD_ENV_OPT");
        }
    }

    #[test_log::test]
    fn test_standard_env_var_parse_opt_none() {
        let env = StandardEnv::new();
        let result: Option<i32> = env.var_parse_opt("MISSING_VAR_888").unwrap();
        assert_eq!(result, None);
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_parse_opt_error() {
        unsafe {
            std::env::set_var("STANDARD_ENV_OPT_ERROR", "not_a_number");
        }

        let env = StandardEnv::new();
        let result: Result<Option<i32>> = env.var_parse_opt("STANDARD_ENV_OPT_ERROR");
        assert!(matches!(result, Err(EnvError::ParseError(_, _))));

        unsafe {
            std::env::remove_var("STANDARD_ENV_OPT_ERROR");
        }
    }

    #[test_log::test]
    #[serial]
    fn test_standard_env_var_exists() {
        unsafe {
            std::env::set_var("STANDARD_ENV_EXISTS", "yes");
        }

        let env = StandardEnv::new();
        assert!(env.var_exists("STANDARD_ENV_EXISTS"));

        unsafe {
            std::env::remove_var("STANDARD_ENV_EXISTS");
        }
        assert!(!env.var_exists("STANDARD_ENV_EXISTS"));
    }

    #[test_log::test]
    fn test_standard_env_vars() {
        let env = StandardEnv::new();
        let vars = env.vars();

        // System environment should have at least some variables
        assert!(!vars.is_empty());
    }

    #[test_log::test]
    #[serial]
    fn test_global_var() {
        unsafe {
            std::env::set_var("GLOBAL_STANDARD_TEST", "global_value");
        }

        assert_eq!(var("GLOBAL_STANDARD_TEST").unwrap(), "global_value");

        unsafe {
            std::env::remove_var("GLOBAL_STANDARD_TEST");
        }
    }

    #[test_log::test]
    fn test_global_var_or() {
        assert_eq!(var_or("MISSING_STANDARD_GLOBAL", "fallback"), "fallback");
    }

    #[test_log::test]
    #[serial]
    fn test_global_var_parse() {
        unsafe {
            std::env::set_var("GLOBAL_STANDARD_NUMBER", "777");
        }

        let result: i32 = var_parse("GLOBAL_STANDARD_NUMBER").unwrap();
        assert_eq!(result, 777);

        unsafe {
            std::env::remove_var("GLOBAL_STANDARD_NUMBER");
        }
    }

    #[test_log::test]
    fn test_global_var_parse_or() {
        let result: i32 = var_parse_or("MISSING_STANDARD_NUMBER", 999);
        assert_eq!(result, 999);
    }

    #[test_log::test]
    #[serial]
    fn test_global_var_parse_opt() {
        unsafe {
            std::env::set_var("OPTIONAL_STANDARD_NUMBER", "555");
        }

        let result: Option<i32> = var_parse_opt("OPTIONAL_STANDARD_NUMBER").unwrap();
        assert_eq!(result, Some(555));

        unsafe {
            std::env::remove_var("OPTIONAL_STANDARD_NUMBER");
        }
    }

    #[test_log::test]
    #[serial]
    fn test_global_var_exists() {
        unsafe {
            std::env::set_var("EXISTS_STANDARD_GLOBAL", "yes");
        }

        assert!(var_exists("EXISTS_STANDARD_GLOBAL"));

        unsafe {
            std::env::remove_var("EXISTS_STANDARD_GLOBAL");
        }
        assert!(!var_exists("EXISTS_STANDARD_GLOBAL"));
    }

    #[test_log::test]
    fn test_global_vars() {
        let all_vars = vars();
        assert!(!all_vars.is_empty());
    }
}
