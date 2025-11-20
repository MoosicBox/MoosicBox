//! Flexible ID types supporting both numeric and string identifiers.
//!
//! This module provides [`Id`] and [`ApiId`] types for identifying music entities across different
//! API sources. Different sources use different ID formats - local/library sources typically use
//! numeric IDs, while external APIs often use string IDs.

use std::num::ParseIntError;

use moosicbox_json_utils::{ParseError, ToValueType};
use moosicbox_parsing_utils::integer_range::parse_integer_ranges;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Error returned when parsing integer ranges fails.
///
/// Re-exported from `moosicbox_parsing_utils` for convenience.
pub use moosicbox_parsing_utils::integer_range::ParseIntegersError;

use crate::ApiSource;

/// Database integration types for ID handling.
///
/// Re-exports database-specific ID types when the `db` feature is enabled.
#[cfg(feature = "db")]
pub use db::*;

/// The type of entity an ID refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum IdType {
    /// Artist entity
    Artist,
    /// Album entity
    Album,
    /// Track entity
    Track,
}

impl std::fmt::Display for IdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Track => "track",
        })
    }
}

/// Represents an ID that is unique within a specific API source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ApiId {
    /// The API source this ID belongs to
    pub source: ApiSource,
    /// The identifier within that source
    pub id: Id,
}

impl ApiId {
    /// Creates a new API ID from a source and ID.
    #[must_use]
    pub const fn new(source: ApiSource, id: Id) -> Self {
        Self { source, id }
    }
}

#[cfg(feature = "openapi")]
impl utoipa::__dev::SchemaReferences for ApiId {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        use utoipa::PartialSchema as _;

        schemas.push(("Id".to_string(), String::schema()));
    }
}

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for ApiId {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        String::schema()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for ApiId {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Id")
    }
}

/// A flexible identifier that can be either a string or numeric value.
///
/// Different API sources use different ID formats - local/library sources
/// typically use numeric IDs, while external APIs often use string IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Id {
    /// String-based identifier
    String(String),
    /// Numeric identifier
    Number(u64),
}

#[cfg(feature = "openapi")]
impl utoipa::__dev::SchemaReferences for Id {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        use utoipa::PartialSchema as _;

        schemas.push(("Id".to_string(), String::schema()));
    }
}

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for Id {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        String::schema()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for Id {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Id")
    }
}

impl Id {
    /// Parses a string into an ID appropriate for the given API source.
    ///
    /// # Panics
    ///
    /// * If the value fails to parse into the relevant type
    #[must_use]
    pub fn from_str(value: &str, source: &ApiSource) -> Self {
        Self::try_from_str(value, source).unwrap()
    }

    /// Attempts to parse a string into an ID appropriate for the given API source.
    ///
    /// # Errors
    ///
    /// * If the value fails to parse into the relevant type
    pub fn try_from_str(value: &str, source: &ApiSource) -> Result<Self, ParseIntError> {
        Ok(if source.is_library() {
            Self::Number(value.parse::<u64>()?)
        } else {
            Self::String(value.to_owned())
        })
    }

    /// Returns the default value for the given API source.
    #[must_use]
    pub fn default_value(source: &ApiSource) -> Self {
        if source.is_library() {
            Self::Number(0)
        } else {
            Self::String(String::new())
        }
    }

    /// Returns `true` if this ID is a number.
    #[must_use]
    pub const fn is_number(&self) -> bool {
        match self {
            Self::String(_) => false,
            Self::Number(_) => true,
        }
    }

    /// Returns the numeric value if this ID is a number.
    #[must_use]
    pub const fn as_u64(&self) -> Option<u64> {
        match self {
            Self::String(_) => None,
            Self::Number(x) => Some(*x),
        }
    }

    /// Returns the numeric value if this ID is a number.
    #[must_use]
    pub const fn as_number(&self) -> Option<u64> {
        match self {
            Self::String(_) => None,
            Self::Number(x) => Some(*x),
        }
    }

    /// Returns `true` if this ID is a string.
    #[must_use]
    pub const fn is_string(&self) -> bool {
        match self {
            Self::String(_) => true,
            Self::Number(_) => false,
        }
    }

    /// Returns the string value if this ID is a string.
    #[must_use]
    pub const fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(x) => Some(x.as_str()),
            Self::Number(_) => None,
        }
    }
}

impl Serialize for Id {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::String(id) => id.serialize(serializer),
            Self::Number(id) => id.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Id {
    /// Deserializes an `Id` from either a string or number.
    ///
    /// # Panics
    ///
    /// * If the value is neither a string nor a number
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = Value::deserialize(deserializer)?;

        if value.is_number() {
            Ok(Self::Number(value.as_u64().unwrap()))
        } else if value.is_string() {
            Ok(Self::String(value.as_str().unwrap().to_string()))
        } else {
            panic!("invalid type")
        }
    }
}

impl ToValueType<Id> for &serde_json::Value {
    /// Converts a JSON value to an `Id`.
    ///
    /// # Errors
    ///
    /// * If the value is neither a string nor a number
    fn to_value_type(self) -> Result<Id, ParseError> {
        if self.is_number() {
            return Ok(Id::Number(
                self.as_u64()
                    .ok_or_else(|| ParseError::ConvertType("Id".into()))?,
            ));
        }
        if self.is_string() {
            return Ok(Id::String(
                self.as_str()
                    .ok_or_else(|| ParseError::ConvertType("Id".into()))?
                    .to_string(),
            ));
        }
        Err(ParseError::ConvertType("Id".into()))
    }
}

#[cfg(feature = "tantivy")]
impl ToValueType<Id> for &tantivy::schema::OwnedValue {
    /// Converts a Tantivy value to an `Id`.
    ///
    /// # Errors
    ///
    /// * If the value is neither a string nor a number
    fn to_value_type(self) -> Result<Id, ParseError> {
        use tantivy::schema::Value;
        Ok(if let Some(id) = self.as_u64() {
            Id::Number(id)
        } else if let Some(id) = self.as_str() {
            Id::String(id.to_owned())
        } else {
            return Err(ParseError::ConvertType("Id".to_string()));
        })
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::Number(0)
    }
}

impl From<&String> for Id {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl From<String> for Id {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

/// Error returned when attempting to convert an ID to the wrong type.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TryFromIdError {
    /// The ID is not the expected type
    #[error("Invalid type. Expected {0}")]
    InvalidType(String),
}

impl TryFrom<Id> for String {
    type Error = TryFromIdError;

    /// Attempts to convert an `Id` to a `String`.
    ///
    /// # Errors
    ///
    /// * If the ID is not a string variant
    fn try_from(value: Id) -> Result<Self, Self::Error> {
        Ok(if let Id::String(string) = value {
            string
        } else {
            return Err(TryFromIdError::InvalidType("String".to_string()));
        })
    }
}

impl TryFrom<&Id> for String {
    type Error = TryFromIdError;

    /// Attempts to convert an `Id` reference to a `String`.
    ///
    /// # Errors
    ///
    /// * If the ID is not a string variant
    fn try_from(value: &Id) -> Result<Self, Self::Error> {
        Ok(if let Id::String(string) = value {
            string.clone()
        } else {
            return Err(TryFromIdError::InvalidType("String".to_string()));
        })
    }
}

impl<'a> TryFrom<&'a Id> for &'a str {
    type Error = TryFromIdError;

    /// Attempts to convert an `Id` reference to a string slice.
    ///
    /// # Errors
    ///
    /// * If the ID is not a string variant
    fn try_from(value: &'a Id) -> Result<Self, Self::Error> {
        Ok(if let Id::String(string) = value {
            string
        } else {
            return Err(TryFromIdError::InvalidType("String".to_string()));
        })
    }
}

impl From<&str> for Id {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<u64> for Id {
    fn from(value: u64) -> Self {
        Self::Number(value)
    }
}

impl TryFrom<Id> for u64 {
    type Error = TryFromIdError;

    /// Attempts to convert an `Id` to a `u64`.
    ///
    /// # Errors
    ///
    /// * If the ID is not a number variant
    fn try_from(value: Id) -> Result<Self, Self::Error> {
        Ok(if let Id::Number(number) = value {
            number
        } else {
            return Err(TryFromIdError::InvalidType("u64".to_string()));
        })
    }
}

impl TryFrom<&Id> for u64 {
    type Error = TryFromIdError;

    /// Attempts to convert an `Id` reference to a `u64`.
    ///
    /// # Errors
    ///
    /// * If the ID is not a number variant
    fn try_from(value: &Id) -> Result<Self, Self::Error> {
        Ok(if let Id::Number(number) = value {
            *number
        } else {
            return Err(TryFromIdError::InvalidType("u64".to_string()));
        })
    }
}

impl From<&u64> for Id {
    fn from(value: &u64) -> Self {
        Self::Number(*value)
    }
}

impl From<i32> for Id {
    fn from(value: i32) -> Self {
        #[allow(clippy::cast_sign_loss)]
        Self::Number(value as u64)
    }
}

impl From<&i32> for Id {
    fn from(value: &i32) -> Self {
        #[allow(clippy::cast_sign_loss)]
        Self::Number(*value as u64)
    }
}

impl TryFrom<Id> for i32 {
    type Error = TryFromIdError;

    /// Attempts to convert an `Id` to an `i32`.
    ///
    /// # Errors
    ///
    /// * If the ID is not a number variant
    fn try_from(value: Id) -> Result<Self, Self::Error> {
        #[allow(clippy::cast_possible_truncation)]
        Ok(if let Id::Number(number) = value {
            number as Self
        } else {
            return Err(TryFromIdError::InvalidType("i32".to_string()));
        })
    }
}

impl TryFrom<&Id> for i32 {
    type Error = TryFromIdError;

    /// Attempts to convert an `Id` reference to an `i32`.
    ///
    /// # Errors
    ///
    /// * If the ID is not a number variant
    fn try_from(value: &Id) -> Result<Self, Self::Error> {
        #[allow(clippy::cast_possible_truncation)]
        Ok(if let Id::Number(number) = value {
            *number as Self
        } else {
            return Err(TryFromIdError::InvalidType("i32".to_string()));
        })
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(string) => f.write_str(string),
            Self::Number(number) => f.write_fmt(format_args!("{number}")),
        }
    }
}

#[cfg(feature = "db")]
mod db {
    use moosicbox_json_utils::{
        ParseError, ToValueType,
        database::{AsModel, AsModelResult, ToValue as _},
    };
    use serde::{Deserialize, Serialize};
    use switchy_database::{AsId, DatabaseValue};

    use super::Id;

    /// Database wrapper for numeric IDs.
    ///
    /// Used for database queries and conversions for library/local content.
    #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct NumberId {
        /// The numeric identifier
        pub id: i32,
    }

    impl AsModel<NumberId> for &switchy_database::Row {
        fn as_model(&self) -> NumberId {
            AsModelResult::as_model(self).unwrap()
        }
    }

    impl AsModelResult<NumberId, ParseError> for &switchy_database::Row {
        /// Converts a database row to a `NumberId`.
        ///
        /// # Errors
        ///
        /// * If the "id" field is missing or cannot be converted to `i32`
        fn as_model(&self) -> Result<NumberId, ParseError> {
            Ok(NumberId {
                id: self.to_value("id")?,
            })
        }
    }

    impl AsId for NumberId {
        fn as_id(&self) -> DatabaseValue {
            #[allow(clippy::cast_lossless)]
            DatabaseValue::Int64(self.id as i64)
        }
    }

    /// Database wrapper for string IDs.
    ///
    /// Used for database queries and conversions for external API content.
    #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct StringId {
        /// The string identifier
        pub id: String,
    }

    impl AsModel<StringId> for &switchy_database::Row {
        fn as_model(&self) -> StringId {
            AsModelResult::as_model(self).unwrap()
        }
    }

    impl AsModelResult<StringId, ParseError> for &switchy_database::Row {
        /// Converts a database row to a `StringId`.
        ///
        /// # Errors
        ///
        /// * If the "id" field is missing or cannot be converted to `String`
        fn as_model(&self) -> Result<StringId, ParseError> {
            Ok(StringId {
                id: self.to_value("id")?,
            })
        }
    }

    impl AsId for StringId {
        fn as_id(&self) -> DatabaseValue {
            DatabaseValue::String(self.id.clone())
        }
    }

    impl From<Id> for switchy_database::DatabaseValue {
        fn from(val: Id) -> Self {
            match val {
                Id::String(x) => Self::String(x),
                Id::Number(x) => Self::UInt64(x),
            }
        }
    }

    impl From<&Id> for switchy_database::DatabaseValue {
        fn from(val: &Id) -> Self {
            match val {
                Id::String(x) => Self::String(x.to_owned()),
                Id::Number(x) => Self::UInt64(*x),
            }
        }
    }

    impl moosicbox_json_utils::MissingValue<Id> for &switchy_database::Row {}
    impl ToValueType<Id> for switchy_database::DatabaseValue {
        /// Converts a database value to an `Id`.
        ///
        /// # Errors
        ///
        /// * If the value cannot be converted to a string or numeric ID
        fn to_value_type(self) -> Result<Id, ParseError> {
            match self {
                Self::String(x) | Self::StringOpt(Some(x)) => Ok(Id::String(x)),
                #[allow(clippy::cast_sign_loss)]
                Self::Int64(x) | Self::Int64Opt(Some(x)) => Ok(Id::Number(x as u64)),
                Self::UInt64(x) | Self::UInt64Opt(Some(x)) => Ok(Id::Number(x)),
                _ => Err(ParseError::ConvertType("Id".into())),
            }
        }
    }
}

/// Parses integer ranges into a vector of numeric IDs.
///
/// Supports ranges like "1,2,5-10" where hyphens indicate ranges.
///
/// # Errors
///
/// * If a number fails to parse to an `Id`
pub fn parse_integer_ranges_to_ids(
    integer_ranges: &str,
) -> std::result::Result<Vec<Id>, ParseIntegersError> {
    Ok(parse_integer_ranges(integer_ranges)?
        .into_iter()
        .map(Into::into)
        .collect::<Vec<Id>>())
}

/// Error returned when parsing ID ranges or sequences.
#[derive(Debug, Error)]
pub enum ParseIdsError {
    /// Failed to parse an ID value
    #[error("Could not parse ids: {0}")]
    ParseId(String),
    /// Range syntax is invalid
    #[error("Unmatched range: {0}")]
    UnmatchedRange(String),
    /// Range is too large (> 100,000 items)
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
}

/// Parses a comma-separated sequence of IDs.
///
/// # Errors
///
/// * If a value fails to parse to an `Id`
pub fn parse_id_sequences(
    ids: &str,
    source: &ApiSource,
) -> std::result::Result<Vec<Id>, ParseIdsError> {
    ids.split(',')
        .map(|id| Id::try_from_str(id, source).map_err(|_| ParseIdsError::ParseId(id.into())))
        .collect::<std::result::Result<Vec<_>, _>>()
}

/// Parses ID ranges and sequences (e.g., "1,2,5-10,20").
///
/// Supports comma-separated IDs and numeric ranges with hyphens.
///
/// # Errors
///
/// * If a value fails to parse to an `Id`
/// * If a range is too large (> 100,000)
pub fn parse_id_ranges(
    id_ranges: &str,
    source: &ApiSource,
) -> std::result::Result<Vec<Id>, ParseIdsError> {
    let default = Id::default_value(source);
    let ranges = if default.is_number() {
        id_ranges.split('-').collect::<Vec<_>>()
    } else {
        vec![id_ranges]
    };

    if ranges.len() == 1 {
        parse_id_sequences(ranges[0], source)
    } else if ranges.len() > 2 && ranges.len() % 2 == 1 {
        Err(ParseIdsError::UnmatchedRange(id_ranges.into()))
    } else {
        let mut i = 0;
        let mut ids = Vec::new();

        while i < ranges.len() {
            let mut start = parse_id_sequences(ranges[i], source)?;
            let start_id = start[start.len() - 1].clone();
            let mut end = parse_id_sequences(ranges[i + 1], source)?;
            let end_id = end[0].clone();

            ids.append(&mut start);

            if let Id::Number(end_id) = end_id
                && let Id::Number(mut start_id) = start_id
            {
                start_id += 1;

                if end_id - start_id > 100_000 {
                    return Err(ParseIdsError::RangeTooLarge(format!(
                        "{}-{}",
                        start_id - 1,
                        end_id,
                    )));
                }

                while start_id < end_id {
                    ids.push(Id::Number(start_id));
                    start_id += 1;
                }
            }

            ids.append(&mut end);

            i += 2;
        }

        Ok(ids)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::{
        ApiSource,
        id::{Id, parse_id_ranges},
    };

    #[test_log::test]
    fn can_parse_number_track_id_ranges() {
        let result = parse_id_ranges("1,2,3,5-10,450", &ApiSource::library()).unwrap();

        assert_eq!(
            result,
            vec![
                Id::Number(1),
                Id::Number(2),
                Id::Number(3),
                Id::Number(5),
                Id::Number(6),
                Id::Number(7),
                Id::Number(8),
                Id::Number(9),
                Id::Number(10),
                Id::Number(450),
            ]
        );
    }

    #[test_log::test]
    fn can_parse_string_track_id_ranges() {
        let result =
            parse_id_ranges("a,b,aaa,bbb,c-d,f", &ApiSource::register("bob", "bob")).unwrap();

        assert_eq!(
            result,
            vec![
                Id::String("a".into()),
                Id::String("b".into()),
                Id::String("aaa".into()),
                Id::String("bbb".into()),
                Id::String("c-d".into()),
                Id::String("f".into()),
            ]
        );
    }
}
