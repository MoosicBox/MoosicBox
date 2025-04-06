use std::str::FromStr as _;

use moosicbox_database::{
    AsId, DatabaseValue, boxed,
    profiles::LibraryDatabase,
    query::{FilterableQuery as _, SortDirection, where_not_eq},
};
use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::{AsModel, AsModelResult, DatabaseFetchError, ToValue as _},
};

use crate::{AlbumVersionQuality, ApiSource, AudioFormat, TrackApiSource, TrackSize};

impl moosicbox_json_utils::MissingValue<ApiSource> for &moosicbox_database::Row {}
impl ToValueType<ApiSource> for DatabaseValue {
    fn to_value_type(self) -> Result<ApiSource, ParseError> {
        ApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ApiSource".into()))
    }
}

impl AsModel<AlbumVersionQuality> for &moosicbox_database::Row {
    fn as_model(&self) -> AlbumVersionQuality {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModel<TrackSize> for &moosicbox_database::Row {
    fn as_model(&self) -> TrackSize {
        AsModelResult::as_model(self).unwrap()
    }
}

impl ToValueType<TrackSize> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<TrackSize, ParseError> {
        Ok(TrackSize {
            id: self.to_value("id")?,
            track_id: self.to_value("track_id")?,
            bytes: self.to_value("bytes")?,
            format: self.to_value("format")?,
        })
    }
}

impl AsModelResult<TrackSize, ParseError> for &moosicbox_database::Row {
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
        DatabaseValue::Number(self.id as i64)
    }
}

impl moosicbox_json_utils::MissingValue<AlbumVersionQuality> for &moosicbox_database::Row {}
impl ToValueType<AlbumVersionQuality> for &moosicbox_database::Row {
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

impl AsModelResult<AlbumVersionQuality, ParseError> for &moosicbox_database::Row {
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
            where_not_eq("tracks.source", TrackApiSource::Local.as_ref())
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
