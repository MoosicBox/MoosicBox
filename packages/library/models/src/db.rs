//! Database integration for library models.
//!
//! This module implements trait conversions for reading and writing library entities
//! to and from database rows. It provides implementations for:
//!
//! * Row-to-model conversions (`AsModel`, `ToValueType`)
//! * Model-to-row conversions (`AsId`)
//! * Complex query operations with joined data (`AsModelQuery`, `AsModelResultMapped`)
//!
//! # Main Functions
//!
//! * [`get_album_version_qualities`] - Retrieves all quality versions for an album from the database
//!
//! # Example
//!
//! ```rust,ignore
//! # use moosicbox_library_models::db::get_album_version_qualities;
//! # async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! let versions = get_album_version_qualities(db, 123).await?;
//! # Ok(())
//! # }
//! ```

use std::str::FromStr as _;

use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType,
    database::{
        AsModel, AsModelQuery, AsModelResult, AsModelResultMapped, DatabaseFetchError, ToValue as _,
    },
};
use moosicbox_music_models::{
    AlbumSource, AlbumVersionQuality, ApiSource, ApiSources, AudioFormat, TrackApiSource,
};
use switchy_database::{
    AsId, Database, DatabaseValue, profiles::LibraryDatabase, query::FilterableQuery as _,
};

use crate::{LibraryAlbum, LibraryAlbumType, LibraryArtist, LibraryTrack, sort_album_versions};

impl AsId for LibraryTrack {
    /// # Panics
    ///
    /// * If the track ID cannot be converted to `i64`
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Int64(self.id.try_into().unwrap())
    }
}

impl AsModel<LibraryArtist> for &switchy_database::Row {
    /// # Panics
    ///
    /// * If the row cannot be converted to a `LibraryArtist`
    fn as_model(&self) -> LibraryArtist {
        AsModelResult::as_model(self).unwrap()
    }
}

impl ToValueType<LibraryArtist> for &switchy_database::Row {
    fn to_value_type(self) -> Result<LibraryArtist, ParseError> {
        let id = self.to_value("id")?;

        Ok(LibraryArtist {
            id,
            title: self.to_value("title")?,
            cover: self.to_value("cover")?,
            api_sources: self
                .to_value::<ApiSources>("api_sources")
                .unwrap_or_default()
                .with_source(ApiSource::library(), id.into()),
        })
    }
}

impl AsModelResult<LibraryArtist, ParseError> for &switchy_database::Row {
    fn as_model(&self) -> Result<LibraryArtist, ParseError> {
        let id = self.to_value("id")?;

        Ok(LibraryArtist {
            id,
            title: self.to_value("title")?,
            cover: self.to_value("cover")?,
            api_sources: self
                .to_value::<ApiSources>("api_sources")
                .unwrap_or_default()
                .with_source(ApiSource::library(), id.into()),
        })
    }
}

impl AsId for LibraryArtist {
    /// # Panics
    ///
    /// * If the artist ID cannot be converted to `i64`
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Int64(self.id.try_into().unwrap())
    }
}

impl MissingValue<LibraryAlbumType> for &switchy_database::Row {}
impl ToValueType<LibraryAlbumType> for &switchy_database::Row {
    fn to_value_type(self) -> Result<LibraryAlbumType, ParseError> {
        self.get("album_type")
            .ok_or_else(|| ParseError::MissingValue("album_type".into()))?
            .to_value_type()
    }
}
impl ToValueType<LibraryAlbumType> for DatabaseValue {
    fn to_value_type(self) -> Result<LibraryAlbumType, ParseError> {
        LibraryAlbumType::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("AlbumType".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("AlbumType".into()))
    }
}

impl AsModel<LibraryAlbum> for &switchy_database::Row {
    /// # Panics
    ///
    /// * If the row cannot be converted to a `LibraryAlbum`
    fn as_model(&self) -> LibraryAlbum {
        AsModelResult::as_model(self).unwrap()
    }
}

impl MissingValue<LibraryAlbum> for &switchy_database::Row {}
impl ToValueType<LibraryAlbum> for &switchy_database::Row {
    fn to_value_type(self) -> Result<LibraryAlbum, ParseError> {
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;

        let api_sources: ApiSources = self.to_value("api_sources")?;
        let artist_api_sources: Option<ApiSources> = self.to_value("artist_api_sources")?;

        let id = self.to_value("id")?;
        let artist_id = self.to_value("artist_id")?;

        Ok(LibraryAlbum {
            id,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id,
            title: self.to_value("title")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            album_sources: api_sources.with_source(ApiSource::library(), id.into()),
            artist_sources: artist_api_sources
                .unwrap_or_default()
                .with_source(ApiSource::library(), artist_id.into()),
        })
    }
}

impl AsModelResult<LibraryAlbum, ParseError> for &switchy_database::Row {
    fn as_model(&self) -> Result<LibraryAlbum, ParseError> {
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;

        let api_sources: ApiSources = self.to_value("api_sources")?;
        let artist_api_sources: ApiSources = self.to_value("artist_api_sources")?;

        let id = self.to_value("id")?;
        let artist_id = self.to_value("artist_id")?;

        Ok(LibraryAlbum {
            id,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id,
            title: self.to_value("title")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            album_sources: api_sources.with_source(ApiSource::library(), id.into()),
            artist_sources: artist_api_sources.with_source(ApiSource::library(), artist_id.into()),
        })
    }
}

impl AsModelResultMapped<LibraryAlbum, DatabaseFetchError> for Vec<switchy_database::Row> {
    #[allow(clippy::too_many_lines)]
    fn as_model_mapped(&self) -> Result<Vec<LibraryAlbum>, DatabaseFetchError> {
        let mut results: Vec<LibraryAlbum> = vec![];
        let mut last_album_id = 0;

        for row in self {
            let album_id: u64 = row
                .get("album_id")
                .ok_or(DatabaseFetchError::InvalidRequest)?
                .try_into()
                .map_err(|_| DatabaseFetchError::InvalidRequest)?;

            if album_id != last_album_id {
                if let Some(album) = results.last_mut() {
                    log::trace!(
                        "Sorting versions for album id={} count={}",
                        album.id,
                        album.versions.len()
                    );
                    sort_album_versions(&mut album.versions);
                }
                match row.to_value_type() {
                    Ok(album) => {
                        results.push(album);
                    }
                    Err(err) => {
                        log::error!("Failed to parse Album for album id={album_id}: {err:?}");
                        continue;
                    }
                }
                last_album_id = album_id;
            }

            if let Some(album) = results.last_mut() {
                if let Some(source) = row.get("source") {
                    match row.to_value_type() {
                        Ok(version) => {
                            album.versions.push(version);
                            log::trace!(
                                "Added version to album id={} count={}",
                                album.id,
                                album.versions.len()
                            );
                        }
                        Err(err) => {
                            log::error!(
                                "Failed to parse AlbumVersionQuality for album id={} ({source:?}): {err:?}",
                                album.id
                            );
                        }
                    }
                } else {
                    for source in album
                        .album_sources
                        .iter()
                        .filter(|x| !x.source.is_library())
                    {
                        album.versions.push(AlbumVersionQuality {
                            format: None,
                            bit_depth: None,
                            sample_rate: None,
                            channels: None,
                            source: source.source.clone().into(),
                        });
                        log::trace!(
                            "Added {} version to album id={} count={}",
                            source.source,
                            album.id,
                            album.versions.len()
                        );
                    }
                }
            }
        }

        if let Some(album) = results.last_mut() {
            log::trace!(
                "Sorting versions for last album id={} count={}",
                album.id,
                album.versions.len()
            );
            sort_album_versions(&mut album.versions);
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl AsModelQuery<LibraryAlbum> for &switchy_database::Row {
    async fn as_model_query(
        &self,
        db: std::sync::Arc<Box<dyn Database>>,
    ) -> Result<LibraryAlbum, DatabaseFetchError> {
        let api_sources: ApiSources = self.to_value("api_sources")?;
        let artist_api_sources: ApiSources = self.to_value("artist_api_sources")?;

        let id = self.to_value("id")?;
        let artist_id = self.to_value("artist_id")?;
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;

        Ok(LibraryAlbum {
            id,
            artist: self
                .to_value::<Option<String>>("artist")?
                .unwrap_or_default(),
            artist_id,
            title: self.to_value("title")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: get_album_version_qualities(&db.into(), id).await?,
            album_sources: api_sources.with_source(ApiSource::library(), id.into()),
            artist_sources: artist_api_sources.with_source(ApiSource::library(), artist_id.into()),
        })
    }
}

impl AsId for LibraryAlbum {
    /// # Panics
    ///
    /// * If the album ID cannot be converted to `i64`
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Int64(self.id.try_into().unwrap())
    }
}

impl AsModel<LibraryTrack> for &switchy_database::Row {
    /// # Panics
    ///
    /// * If the row cannot be converted to a `LibraryTrack`
    fn as_model(&self) -> LibraryTrack {
        AsModelResult::as_model(self).unwrap()
    }
}

impl ToValueType<LibraryTrack> for &switchy_database::Row {
    /// # Panics
    ///
    /// * If the `source` field contains an invalid `TrackApiSource` value
    fn to_value_type(self) -> Result<LibraryTrack, ParseError> {
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;
        let id = self.to_value("id")?;

        Ok(LibraryTrack {
            id,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released").unwrap_or_default(),
            date_added: self.to_value("date_added").unwrap_or_default(),
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id").unwrap_or_default(),
            file: self.to_value("file")?,
            artwork: self.to_value("artwork").unwrap_or_default(),
            blur: self.to_value("blur").unwrap_or_default(),
            bytes: self.to_value("bytes").unwrap_or_default(),
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            audio_bitrate: self.to_value("audio_bitrate").unwrap_or_default(),
            overall_bitrate: self.to_value("overall_bitrate").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate").unwrap_or_default(),
            channels: self.to_value("channels").unwrap_or_default(),
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .expect("Missing source"),
            api_source: ApiSource::library(),
            api_sources: self
                .to_value::<ApiSources>("api_sources")
                .unwrap_or_default()
                .with_source(ApiSource::library(), id.into()),
        })
    }
}

impl AsModelResult<LibraryTrack, ParseError> for &switchy_database::Row {
    /// # Panics
    ///
    /// * If the `source` field contains an invalid `TrackApiSource` value
    fn as_model(&self) -> Result<LibraryTrack, ParseError> {
        let album_type: Option<LibraryAlbumType> = self.to_value("album_type")?;
        let id = self.to_value("id")?;

        Ok(LibraryTrack {
            id,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            album_type: album_type.unwrap_or_default(),
            date_released: self.to_value("date_released").unwrap_or_default(),
            date_added: self.to_value("date_added").unwrap_or_default(),
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id").unwrap_or_default(),
            file: self.to_value("file")?,
            artwork: self.to_value("artwork").unwrap_or_default(),
            blur: self.to_value("blur").unwrap_or_default(),
            bytes: self.to_value("bytes").unwrap_or_default(),
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            audio_bitrate: self.to_value("audio_bitrate").unwrap_or_default(),
            overall_bitrate: self.to_value("overall_bitrate").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate").unwrap_or_default(),
            channels: self.to_value("channels").unwrap_or_default(),
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .expect("Missing source"),
            api_source: ApiSource::library(),
            api_sources: self
                .to_value::<ApiSources>("api_sources")
                .unwrap_or_default()
                .with_source(ApiSource::library(), id.into()),
        })
    }
}

/// Retrieves all available quality versions for an album from the database.
///
/// Queries the database for distinct track quality information (bit depth, sample rate,
/// channels, format) associated with the album and returns them sorted by quality.
///
/// # Errors
///
/// * If fails to get the data from the database
/// * If fails to parse the data from the database
pub async fn get_album_version_qualities(
    db: &LibraryDatabase,
    album_id: u64,
) -> Result<Vec<AlbumVersionQuality>, DatabaseFetchError> {
    let mut versions: Vec<AlbumVersionQuality> = db
        .select("albums")
        .distinct()
        .columns(&[
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "tracks.format",
            "tracks.source",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .where_eq("albums.id", album_id)
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
