//! Core functionality for the test workspace

pub fn core_function() -> &'static str {
    "core"
}

#[cfg(feature = "json")]
pub mod json {
    pub fn serialize() -> &'static str {
        "json serialization"
    }
}

#[cfg(feature = "database")]
pub mod database {
    pub fn connect() -> &'static str {
        "database connection"
    }
}
