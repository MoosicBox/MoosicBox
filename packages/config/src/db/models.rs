//! Database models for `MoosicBox` configuration.
//!
//! This module contains data structures for configuration entities stored in the database,
//! along with their database serialization implementations.

use moosicbox_json_utils::{ParseError, ToValueType, database::ToValue};

/// Represents a `MoosicBox` profile stored in the database.
pub struct Profile {
    /// Unique identifier for the profile
    pub id: u64,
    /// Name of the profile
    pub name: String,
}

impl ToValueType<Profile> for &switchy_database::Row {
    fn to_value_type(self) -> Result<Profile, ParseError> {
        Ok(Profile {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
        })
    }
}
