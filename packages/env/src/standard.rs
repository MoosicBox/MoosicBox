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
pub fn vars() -> BTreeMap<String, String> {
    PROVIDER.vars()
}
