//! Switchy environment variable access with pluggable backends.
//!
//! This crate provides a unified interface for accessing environment variables with support
//! for both standard system environment variables and a simulator mode for testing.
//!
//! # Features
//!
//! * **standard**: Uses `std::env` for real environment variable access
//! * **simulator**: Provides a configurable environment for testing with deterministic defaults
//!
//! # Usage
//!
//! With the `standard` feature (default), access environment variables:
//!
//! ```rust
//! # #[cfg(feature = "std")]
//! # {
//! use switchy_env::{var, var_parse};
//!
//! # unsafe { std::env::set_var("PORT", "8080"); }
//! // Get a variable as a string
//! let port_str = var("PORT").unwrap();
//!
//! // Parse a variable as a specific type
//! let port: u16 = var_parse("PORT").unwrap();
//! # }
//! ```
//!
//! With the `simulator` feature, configure variables for testing:
//!
//! ```rust,ignore
//! use switchy_env::{set_var, var, reset};
//!
//! // Set a test variable
//! set_var("DATABASE_URL", "sqlite::memory:");
//!
//! // Access it like normal
//! let db_url = var("DATABASE_URL").unwrap();
//!
//! // Reset to defaults
//! reset();
//! ```
//!
//! # Custom Providers
//!
//! Implement the [`EnvProvider`] trait to create custom environment variable sources:
//!
//! ```rust
//! use switchy_env::{EnvProvider, EnvError, Result};
//! use std::collections::BTreeMap;
//!
//! struct CustomEnv;
//!
//! impl EnvProvider for CustomEnv {
//!     fn var(&self, name: &str) -> Result<String> {
//!         // Custom logic here
//!         Err(EnvError::NotFound(name.to_string()))
//!     }
//!
//!     fn vars(&self) -> BTreeMap<String, String> {
//!         BTreeMap::new()
//!     }
//! }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;

/// Standard system environment variable access.
///
/// This module provides access to real system environment variables via `std::env`.
/// Enabled with the `std` feature (default).
#[cfg(feature = "std")]
pub mod standard;

/// Simulator environment for testing.
///
/// This module provides a configurable environment with deterministic defaults
/// for testing. Enabled with the `simulator` feature (default).
#[cfg(feature = "simulator")]
pub mod simulator;

/// Error types for environment variable operations.
///
/// These errors can occur when accessing or parsing environment variables.
#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    /// Environment variable was not found
    #[error("Environment variable '{0}' not found")]
    NotFound(String),
    /// Environment variable has an invalid value
    #[error("Environment variable '{0}' has invalid value: {1}")]
    InvalidValue(String, String),
    /// Failed to parse environment variable value
    #[error("Parse error for '{0}': {1}")]
    ParseError(String, String),
}

/// Result type for environment variable operations.
///
/// A convenience type alias that uses [`EnvError`] as the error type.
pub type Result<T> = std::result::Result<T, EnvError>;

/// Trait for environment variable access.
///
/// This trait provides a unified interface for accessing environment variables
/// from different sources (system environment, simulator, custom implementations).
/// All providers must be thread-safe (`Send + Sync`).
///
/// # Examples
///
/// Implementing a custom provider:
///
/// ```rust
/// use switchy_env::{EnvProvider, EnvError, Result};
/// use std::collections::BTreeMap;
///
/// struct CustomEnv {
///     vars: BTreeMap<String, String>,
/// }
///
/// impl EnvProvider for CustomEnv {
///     fn var(&self, name: &str) -> Result<String> {
///         self.vars.get(name)
///             .cloned()
///             .ok_or_else(|| EnvError::NotFound(name.to_string()))
///     }
///
///     fn vars(&self) -> BTreeMap<String, String> {
///         self.vars.clone()
///     }
/// }
/// ```
pub trait EnvProvider: Send + Sync {
    /// Get an environment variable as a string
    ///
    /// # Errors
    ///
    /// * If the environment variable is not found
    fn var(&self, name: &str) -> Result<String>;

    /// Get an environment variable with a default value
    fn var_or(&self, name: &str, default: &str) -> String {
        self.var(name).unwrap_or_else(|_| default.to_string())
    }

    /// Get an environment variable parsed as a specific type
    ///
    /// # Errors
    ///
    /// * If the environment variable is not found
    /// * If the environment variable value cannot be parsed to the target type
    fn var_parse<T>(&self, name: &str) -> Result<T>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        let value = self.var(name)?;
        value
            .parse::<T>()
            .map_err(|e| EnvError::ParseError(name.to_string(), e.to_string()))
    }

    /// Get an environment variable parsed with a default value
    fn var_parse_or<T>(&self, name: &str, default: T) -> T
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        self.var_parse(name).unwrap_or(default)
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
    /// * If the environment variable fails to parse
    fn var_parse_opt<T>(&self, name: &str) -> Result<Option<T>>
    where
        T: std::str::FromStr,
        T::Err: std::fmt::Display,
    {
        match self.var(name) {
            Ok(value) => value
                .parse::<T>()
                .map(Some)
                .map_err(|e| EnvError::ParseError(name.to_string(), e.to_string())),
            Err(EnvError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Check if an environment variable exists
    fn var_exists(&self, name: &str) -> bool {
        self.var(name).is_ok()
    }

    /// Get all environment variables
    fn vars(&self) -> BTreeMap<String, String>;
}

#[allow(unused)]
macro_rules! impl_env {
    ($module:ident $(,)?) => {
        pub use $module::{var, var_exists, var_or, var_parse, var_parse_opt, var_parse_or, vars};
    };
}

#[cfg(feature = "simulator")]
impl_env!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "std"))]
impl_env!(standard);

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEnvProvider {
        vars: BTreeMap<String, String>,
    }

    impl EnvProvider for TestEnvProvider {
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

    #[test_log::test]
    fn test_custom_env_provider_implementation() {
        let mut vars = BTreeMap::new();
        vars.insert("KEY1".to_string(), "value1".to_string());
        vars.insert("KEY2".to_string(), "value2".to_string());

        let provider = TestEnvProvider { vars };

        assert_eq!(provider.var("KEY1").unwrap(), "value1");
        assert_eq!(provider.var("KEY2").unwrap(), "value2");
        assert!(matches!(provider.var("KEY3"), Err(EnvError::NotFound(_))));
    }

    #[test_log::test]
    fn test_env_provider_var_or() {
        let provider = TestEnvProvider {
            vars: BTreeMap::new(),
        };

        assert_eq!(provider.var_or("MISSING", "default"), "default");
    }

    #[test_log::test]
    fn test_env_provider_var_or_with_existing() {
        let mut vars = BTreeMap::new();
        vars.insert("EXISTS".to_string(), "actual".to_string());

        let provider = TestEnvProvider { vars };

        assert_eq!(provider.var_or("EXISTS", "default"), "actual");
    }

    #[test_log::test]
    fn test_env_provider_var_parse_success() {
        let mut vars = BTreeMap::new();
        vars.insert("NUMBER".to_string(), "42".to_string());

        let provider = TestEnvProvider { vars };

        let result: i32 = provider.var_parse("NUMBER").unwrap();
        assert_eq!(result, 42);
    }

    #[test_log::test]
    fn test_env_provider_var_parse_error() {
        let mut vars = BTreeMap::new();
        vars.insert("NOT_NUMBER".to_string(), "abc".to_string());

        let provider = TestEnvProvider { vars };

        let result: Result<i32> = provider.var_parse("NOT_NUMBER");
        assert!(matches!(result, Err(EnvError::ParseError(_, _))));
    }

    #[test_log::test]
    fn test_env_provider_var_parse_missing() {
        let provider = TestEnvProvider {
            vars: BTreeMap::new(),
        };

        let result: Result<i32> = provider.var_parse("MISSING");
        assert!(matches!(result, Err(EnvError::NotFound(_))));
    }

    #[test_log::test]
    fn test_env_provider_var_parse_or_success() {
        let mut vars = BTreeMap::new();
        vars.insert("NUMBER".to_string(), "100".to_string());

        let provider = TestEnvProvider { vars };

        let result: i32 = provider.var_parse_or("NUMBER", 42);
        assert_eq!(result, 100);
    }

    #[test_log::test]
    fn test_env_provider_var_parse_or_with_parse_error() {
        let mut vars = BTreeMap::new();
        vars.insert("NOT_NUMBER".to_string(), "xyz".to_string());

        let provider = TestEnvProvider { vars };

        let result: i32 = provider.var_parse_or("NOT_NUMBER", 42);
        assert_eq!(result, 42);
    }

    #[test_log::test]
    fn test_env_provider_var_parse_or_with_missing() {
        let provider = TestEnvProvider {
            vars: BTreeMap::new(),
        };

        let result: i32 = provider.var_parse_or("MISSING", 42);
        assert_eq!(result, 42);
    }

    #[test_log::test]
    fn test_env_provider_var_parse_opt_some() {
        let mut vars = BTreeMap::new();
        vars.insert("NUMBER".to_string(), "123".to_string());

        let provider = TestEnvProvider { vars };

        let result: Option<i32> = provider.var_parse_opt("NUMBER").unwrap();
        assert_eq!(result, Some(123));
    }

    #[test_log::test]
    fn test_env_provider_var_parse_opt_none() {
        let provider = TestEnvProvider {
            vars: BTreeMap::new(),
        };

        let result: Option<i32> = provider.var_parse_opt("MISSING").unwrap();
        assert_eq!(result, None);
    }

    #[test_log::test]
    fn test_env_provider_var_parse_opt_parse_error() {
        let mut vars = BTreeMap::new();
        vars.insert("NOT_NUMBER".to_string(), "not_an_int".to_string());

        let provider = TestEnvProvider { vars };

        let result: Result<Option<i32>> = provider.var_parse_opt("NOT_NUMBER");
        assert!(matches!(result, Err(EnvError::ParseError(_, _))));
    }

    #[test_log::test]
    fn test_env_provider_var_exists_true() {
        let mut vars = BTreeMap::new();
        vars.insert("EXISTS".to_string(), "yes".to_string());

        let provider = TestEnvProvider { vars };

        assert!(provider.var_exists("EXISTS"));
    }

    #[test_log::test]
    fn test_env_provider_var_exists_false() {
        let provider = TestEnvProvider {
            vars: BTreeMap::new(),
        };

        assert!(!provider.var_exists("MISSING"));
    }

    #[test_log::test]
    fn test_env_provider_vars() {
        let mut vars = BTreeMap::new();
        vars.insert("KEY1".to_string(), "value1".to_string());
        vars.insert("KEY2".to_string(), "value2".to_string());

        let provider = TestEnvProvider { vars: vars.clone() };

        let result = provider.vars();
        assert_eq!(result, vars);
    }

    #[test_log::test]
    fn test_result_type_alias() {
        let ok_result: Result<String> = Ok("test".to_string());
        assert!(ok_result.is_ok());

        let err_result: Result<String> = Err(EnvError::NotFound("VAR".to_string()));
        assert!(err_result.is_err());
    }

    /// Provider that returns `InvalidValue` errors for specific variables
    struct InvalidValueProvider {
        invalid_vars: std::collections::BTreeSet<String>,
    }

    impl EnvProvider for InvalidValueProvider {
        fn var(&self, name: &str) -> Result<String> {
            if self.invalid_vars.contains(name) {
                Err(EnvError::InvalidValue(
                    name.to_string(),
                    "contains null byte".to_string(),
                ))
            } else {
                Err(EnvError::NotFound(name.to_string()))
            }
        }

        fn vars(&self) -> BTreeMap<String, String> {
            BTreeMap::new()
        }
    }

    #[test_log::test]
    fn test_env_provider_var_parse_opt_propagates_invalid_value_error() {
        // This tests the `Err(e) => Err(e)` branch in var_parse_opt that propagates
        // non-NotFound errors (like InvalidValue) from the underlying var() call.
        let mut invalid_vars = std::collections::BTreeSet::new();
        invalid_vars.insert("INVALID_VAR".to_string());

        let provider = InvalidValueProvider { invalid_vars };

        let result: Result<Option<i32>> = provider.var_parse_opt("INVALID_VAR");

        // Should propagate the InvalidValue error, not return Ok(None)
        assert!(matches!(
            result,
            Err(EnvError::InvalidValue(ref name, _)) if name == "INVALID_VAR"
        ));
    }

    #[test_log::test]
    fn test_env_provider_var_parse_propagates_invalid_value_error() {
        // Test that var_parse also handles InvalidValue errors correctly
        let mut invalid_vars = std::collections::BTreeSet::new();
        invalid_vars.insert("INVALID_VAR".to_string());

        let provider = InvalidValueProvider { invalid_vars };

        let result: Result<i32> = provider.var_parse("INVALID_VAR");

        // Should propagate the InvalidValue error
        assert!(matches!(
            result,
            Err(EnvError::InvalidValue(ref name, _)) if name == "INVALID_VAR"
        ));
    }
}
