#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::collections::BTreeMap;

#[cfg(feature = "std")]
pub mod standard;

#[cfg(feature = "simulator")]
pub mod simulator;

/// Environment variable error types
#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    #[error("Environment variable '{0}' not found")]
    NotFound(String),
    #[error("Environment variable '{0}' has invalid value: {1}")]
    InvalidValue(String, String),
    #[error("Parse error for '{0}': {1}")]
    ParseError(String, String),
}

/// Result type for environment operations
pub type Result<T> = std::result::Result<T, EnvError>;

/// Trait for environment variable access
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
