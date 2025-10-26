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
    fn var(&self, name: &str) -> Result<String> {
        let vars = self.vars.read().unwrap();
        vars.get(name)
            .cloned()
            .ok_or_else(|| EnvError::NotFound(name.to_string()))
    }

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

/// Set a variable for testing (simulator only)
pub fn set_var(name: &str, value: &str) {
    PROVIDER.set_var(name, value);
}

/// Remove a variable (simulator only)
pub fn remove_var(name: &str) {
    PROVIDER.remove_var(name);
}

/// Clear all variables (simulator only)
pub fn clear() {
    PROVIDER.clear();
}

/// Reset to defaults (simulator only)
pub fn reset() {
    PROVIDER.reset();
}
