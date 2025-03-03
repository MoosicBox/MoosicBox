use std::num::ParseIntError;

use moosicbox_json_utils::{ParseError, ToValueType};
use moosicbox_parsing_utils::integer_range::parse_integer_ranges;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub use moosicbox_parsing_utils::integer_range::ParseIntegersError;

use crate::ApiSource;

#[cfg(feature = "db")]
pub use db::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum IdType {
    Artist,
    Album,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Id {
    String(String),
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
    /// # Panics
    ///
    /// * If the value fails to parse into the relevant type
    #[must_use]
    pub fn from_str(value: &str, source: ApiSource, id_type: IdType) -> Self {
        Self::try_from_str(value, source, id_type).unwrap()
    }

    /// # Errors
    ///
    /// * If the value fails to parse into the relevant type
    pub fn try_from_str(
        value: &str,
        source: ApiSource,
        id_type: IdType,
    ) -> Result<Self, ParseIntError> {
        Ok(match id_type {
            IdType::Artist => match source {
                ApiSource::Library => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "tidal")]
                ApiSource::Tidal => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "qobuz")]
                ApiSource::Qobuz => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "yt")]
                ApiSource::Yt => Self::String(value.to_owned()),
            },
            IdType::Album => match source {
                ApiSource::Library => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "tidal")]
                ApiSource::Tidal => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "qobuz")]
                ApiSource::Qobuz => Self::String(value.to_owned()),
                #[cfg(feature = "yt")]
                ApiSource::Yt => Self::String(value.to_owned()),
            },
            IdType::Track => match source {
                ApiSource::Library => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "tidal")]
                ApiSource::Tidal => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "qobuz")]
                ApiSource::Qobuz => Self::Number(value.parse::<u64>()?),
                #[cfg(feature = "yt")]
                ApiSource::Yt => Self::String(value.to_owned()),
            },
        })
    }

    #[must_use]
    pub const fn default_value(source: ApiSource, id_type: IdType) -> Self {
        #[allow(clippy::match_same_arms)]
        match id_type {
            IdType::Album => match source {
                ApiSource::Library => Self::Number(0),
                #[cfg(feature = "tidal")]
                ApiSource::Tidal => Self::Number(0),
                #[cfg(feature = "qobuz")]
                ApiSource::Qobuz => Self::String(String::new()),
                #[cfg(feature = "yt")]
                ApiSource::Yt => Self::String(String::new()),
            },
            IdType::Track | IdType::Artist => match source {
                ApiSource::Library => Self::Number(0),
                #[cfg(feature = "tidal")]
                ApiSource::Tidal => Self::Number(0),
                #[cfg(feature = "qobuz")]
                ApiSource::Qobuz => Self::Number(0),
                #[cfg(feature = "yt")]
                ApiSource::Yt => Self::String(String::new()),
            },
        }
    }

    #[must_use]
    pub const fn is_number(&self) -> bool {
        match self {
            Self::String(_) => false,
            Self::Number(_) => true,
        }
    }

    #[must_use]
    pub const fn as_u64(&self) -> Option<u64> {
        match self {
            Self::String(_) => None,
            Self::Number(x) => Some(*x),
        }
    }

    #[must_use]
    pub const fn as_number(&self) -> Option<u64> {
        match self {
            Self::String(_) => None,
            Self::Number(x) => Some(*x),
        }
    }

    #[must_use]
    pub const fn is_string(&self) -> bool {
        match self {
            Self::String(_) => true,
            Self::Number(_) => false,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
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

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TryFromIdError {
    #[error("Invalid type. Expected {0}")]
    InvalidType(String),
}

impl TryFrom<Id> for String {
    type Error = TryFromIdError;

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

    fn try_from(value: &Id) -> Result<Self, Self::Error> {
        Ok(if let Id::String(string) = value {
            string.to_string()
        } else {
            return Err(TryFromIdError::InvalidType("String".to_string()));
        })
    }
}

impl<'a> TryFrom<&'a Id> for &'a str {
    type Error = TryFromIdError;

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
    use moosicbox_database::{AsId, DatabaseValue};
    use moosicbox_json_utils::{
        ParseError, ToValueType,
        database::{AsModel, AsModelResult, ToValue as _},
    };
    use serde::{Deserialize, Serialize};

    use super::Id;

    #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct NumberId {
        pub id: i32,
    }

    impl AsModel<NumberId> for &moosicbox_database::Row {
        fn as_model(&self) -> NumberId {
            AsModelResult::as_model(self).unwrap()
        }
    }

    impl AsModelResult<NumberId, ParseError> for &moosicbox_database::Row {
        fn as_model(&self) -> Result<NumberId, ParseError> {
            Ok(NumberId {
                id: self.to_value("id")?,
            })
        }
    }

    impl AsId for NumberId {
        fn as_id(&self) -> DatabaseValue {
            #[allow(clippy::cast_lossless)]
            DatabaseValue::Number(self.id as i64)
        }
    }

    #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct StringId {
        pub id: String,
    }

    impl AsModel<StringId> for &moosicbox_database::Row {
        fn as_model(&self) -> StringId {
            AsModelResult::as_model(self).unwrap()
        }
    }

    impl AsModelResult<StringId, ParseError> for &moosicbox_database::Row {
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

    impl From<Id> for moosicbox_database::DatabaseValue {
        fn from(val: Id) -> Self {
            match val {
                Id::String(x) => Self::String(x),
                Id::Number(x) => Self::UNumber(x),
            }
        }
    }

    impl From<&Id> for moosicbox_database::DatabaseValue {
        fn from(val: &Id) -> Self {
            match val {
                Id::String(x) => Self::String(x.to_owned()),
                Id::Number(x) => Self::UNumber(*x),
            }
        }
    }

    impl moosicbox_json_utils::MissingValue<Id> for &moosicbox_database::Row {}
    impl ToValueType<Id> for moosicbox_database::DatabaseValue {
        fn to_value_type(self) -> Result<Id, ParseError> {
            match self {
                Self::String(x) | Self::StringOpt(Some(x)) => Ok(Id::String(x)),
                #[allow(clippy::cast_sign_loss)]
                Self::Number(x) | Self::NumberOpt(Some(x)) => Ok(Id::Number(x as u64)),
                Self::UNumber(x) | Self::UNumberOpt(Some(x)) => Ok(Id::Number(x)),
                _ => Err(ParseError::ConvertType("Id".into())),
            }
        }
    }
}

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

#[derive(Debug, Error)]
pub enum ParseIdsError {
    #[error("Could not parse ids: {0}")]
    ParseId(String),
    #[error("Unmatched range: {0}")]
    UnmatchedRange(String),
    #[error("Range too large: {0}")]
    RangeTooLarge(String),
}

/// # Errors
///
/// * If a value fails to parse to an `Id`
pub fn parse_id_sequences(
    ids: &str,
    source: ApiSource,
    id_type: IdType,
) -> std::result::Result<Vec<Id>, ParseIdsError> {
    ids.split(',')
        .map(|id| {
            Id::try_from_str(id, source, id_type).map_err(|_| ParseIdsError::ParseId(id.into()))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
}

/// # Errors
///
/// * If a value fails to parse to an `Id`
/// * If a range is too large (> 100,000)
pub fn parse_id_ranges(
    id_ranges: &str,
    source: ApiSource,
    id_type: IdType,
) -> std::result::Result<Vec<Id>, ParseIdsError> {
    let default = Id::default_value(source, id_type);
    let ranges = if default.is_number() {
        id_ranges.split('-').collect::<Vec<_>>()
    } else {
        vec![id_ranges]
    };

    if ranges.len() == 1 {
        parse_id_sequences(ranges[0], source, id_type)
    } else if ranges.len() > 2 && ranges.len() % 2 == 1 {
        Err(ParseIdsError::UnmatchedRange(id_ranges.into()))
    } else {
        let mut i = 0;
        let mut ids = Vec::new();

        while i < ranges.len() {
            let mut start = parse_id_sequences(ranges[i], source, id_type)?;
            let start_id = start[start.len() - 1].clone();
            let mut end = parse_id_sequences(ranges[i + 1], source, id_type)?;
            let end_id = end[0].clone();

            ids.append(&mut start);

            if let Id::Number(end_id) = end_id {
                if let Id::Number(mut start_id) = start_id {
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
        id::{Id, IdType, parse_id_ranges},
    };

    #[test_log::test]
    fn can_parse_number_track_id_ranges() {
        let result = parse_id_ranges("1,2,3,5-10,450", ApiSource::Library, IdType::Track).unwrap();

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

    #[cfg(feature = "yt")]
    #[test_log::test]
    fn can_parse_string_track_id_ranges() {
        let result = parse_id_ranges("a,b,aaa,bbb,c-d,f", ApiSource::Yt, IdType::Track).unwrap();

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
