//! Database integration for model types.
//!
//! This module provides database value conversions, query support, and model deserialization
//! for the `switchy_database` library. It includes implementations for converting between
//! database rows and model types, as well as specialized query functions.

use std::str::FromStr as _;

use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::{AsModel, AsModelResult, DatabaseFetchError, ToValue as _},
};
use switchy_database::{
    AsId, DatabaseValue, boxed,
    profiles::LibraryDatabase,
    query::{FilterableQuery as _, SortDirection, where_not_eq},
};

use crate::{AlbumVersionQuality, ApiSource, ApiSources, AudioFormat, TrackApiSource, TrackSize};

impl moosicbox_json_utils::MissingValue<ApiSource> for &switchy_database::Row {}
impl ToValueType<ApiSource> for DatabaseValue {
    /// Converts a database value to an `ApiSource`.
    ///
    /// # Errors
    ///
    /// * If the value is not a string
    /// * If the string doesn't match any registered API source
    fn to_value_type(self) -> Result<ApiSource, ParseError> {
        ApiSource::try_from(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiSource".into()))?,
        )
        .map_err(|e| ParseError::ConvertType(format!("ApiSource: {e:?}")))
    }
}

impl AsModel<AlbumVersionQuality> for &switchy_database::Row {
    fn as_model(&self) -> AlbumVersionQuality {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModel<TrackSize> for &switchy_database::Row {
    fn as_model(&self) -> TrackSize {
        AsModelResult::as_model(self).unwrap()
    }
}

impl ToValueType<TrackSize> for &switchy_database::Row {
    /// Converts a database row to a `TrackSize`.
    ///
    /// # Errors
    ///
    /// * If any required field is missing or cannot be converted
    fn to_value_type(self) -> Result<TrackSize, ParseError> {
        Ok(TrackSize {
            id: self.to_value("id")?,
            track_id: self.to_value("track_id")?,
            bytes: self.to_value("bytes")?,
            format: self.to_value("format")?,
        })
    }
}

impl AsModelResult<TrackSize, ParseError> for &switchy_database::Row {
    /// Converts a database row to a `TrackSize`.
    ///
    /// # Errors
    ///
    /// * If any required field is missing or cannot be converted
    fn as_model(&self) -> Result<TrackSize, ParseError> {
        Ok(TrackSize {
            id: self.to_value("id")?,
            track_id: self.to_value("track_id")?,
            bytes: self.to_value("bytes")?,
            format: self.to_value("format")?,
        })
    }
}

impl AsId for TrackSize {
    fn as_id(&self) -> DatabaseValue {
        #[allow(clippy::cast_possible_wrap)]
        DatabaseValue::Int64(self.id as i64)
    }
}

impl moosicbox_json_utils::MissingValue<AlbumVersionQuality> for &switchy_database::Row {}
impl ToValueType<AlbumVersionQuality> for &switchy_database::Row {
    /// Converts a database row to an `AlbumVersionQuality`.
    ///
    /// # Errors
    ///
    /// * If any required field is missing or cannot be converted
    /// * If the format string is invalid
    /// * If the source string is invalid
    fn to_value_type(self) -> Result<AlbumVersionQuality, ParseError> {
        Ok(AlbumVersionQuality {
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate")?,
            channels: self.to_value("channels")?,
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .map_err(|e| ParseError::ConvertType(format!("Invalid source: {e:?}")))?,
        })
    }
}

impl AsModelResult<AlbumVersionQuality, ParseError> for &switchy_database::Row {
    /// Converts a database row to an `AlbumVersionQuality`.
    ///
    /// # Errors
    ///
    /// * If any required field is missing or cannot be converted
    /// * If the format string is invalid
    /// * If the source string is invalid
    fn as_model(&self) -> Result<AlbumVersionQuality, ParseError> {
        Ok(AlbumVersionQuality {
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate")?,
            channels: self.to_value("channels")?,
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .map_err(|e| ParseError::ConvertType(format!("Invalid source: {e:?}")))?,
        })
    }
}

/// Retrieves all available album version qualities for the given album IDs.
///
/// Queries the database for distinct quality combinations (format, bit depth, sample rate,
/// channels) available for each album, sorted by quality (higher sample rate and bit depth first).
///
/// # Errors
///
/// * If fails to get the data from the database
/// * If fails to parse the data from the database
pub async fn get_all_album_version_qualities(
    db: &LibraryDatabase,
    album_ids: Vec<u64>,
) -> Result<Vec<AlbumVersionQuality>, DatabaseFetchError> {
    let mut versions: Vec<AlbumVersionQuality> = db
        .select("albums")
        .distinct()
        .columns(&[
            "albums.id as album_id",
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "track_sizes.format",
            "tracks.source",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .where_in("albums.id", album_ids)
        .sort("albums.id", SortDirection::Desc)
        .where_or(boxed![
            where_not_eq("track_sizes.format", AudioFormat::Source.as_ref()),
            where_not_eq("tracks.source", TrackApiSource::Local.to_string())
        ])
        .execute(&**db)
        .await?
        .to_value_type()?;

    versions.sort_by(|a: &AlbumVersionQuality, b: &AlbumVersionQuality| {
        b.sample_rate
            .unwrap_or_default()
            .cmp(&a.sample_rate.unwrap_or_default())
    });
    versions.sort_by(|a: &AlbumVersionQuality, b: &AlbumVersionQuality| {
        b.bit_depth
            .unwrap_or_default()
            .cmp(&a.bit_depth.unwrap_or_default())
    });

    Ok(versions)
}

impl moosicbox_json_utils::MissingValue<ApiSources> for &switchy_database::Row {}
impl moosicbox_json_utils::MissingValue<ApiSources> for switchy_database::DatabaseValue {}
impl ToValueType<ApiSources> for switchy_database::DatabaseValue {
    /// Converts a database value to `ApiSources`.
    ///
    /// # Errors
    ///
    /// * If the value is not a string
    /// * If JSON deserialization fails
    fn to_value_type(self) -> Result<ApiSources, ParseError> {
        serde_json::from_str(self.as_str().ok_or_else(|| {
            ParseError::MissingValue("ApiSources: value is not a string".to_string())
        })?)
        .map_err(|_| ParseError::ConvertType("ApiSources".into()))
    }
}

impl From<&ApiSource> for DatabaseValue {
    fn from(value: &ApiSource) -> Self {
        Self::String(value.id.clone())
    }
}

impl From<ApiSource> for DatabaseValue {
    fn from(value: ApiSource) -> Self {
        Self::String(value.id)
    }
}

impl moosicbox_json_utils::MissingValue<TrackApiSource> for &switchy_database::Row {}
impl ToValueType<TrackApiSource> for &switchy_database::Row {
    /// Converts a database row to a `TrackApiSource`.
    ///
    /// # Errors
    ///
    /// * If the "origin" field is missing
    /// * If the value cannot be converted to `TrackApiSource`
    fn to_value_type(self) -> Result<TrackApiSource, ParseError> {
        self.get("origin")
            .ok_or_else(|| ParseError::MissingValue("origin".into()))?
            .to_value_type()
    }
}
impl ToValueType<TrackApiSource> for DatabaseValue {
    /// Converts a database value to a `TrackApiSource`.
    ///
    /// # Errors
    ///
    /// * If the value is not a string
    /// * If the string doesn't match the expected format
    fn to_value_type(self) -> Result<TrackApiSource, ParseError> {
        TrackApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackApiSource".into()))
    }
}

impl AsId for TrackApiSource {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.to_string())
    }
}
