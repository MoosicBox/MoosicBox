//! Database operations for library metadata.
//!
//! This module provides database access functions for managing library data including
//! artists, albums, tracks, and their relationships. It handles CRUD operations and
//! complex queries for the library database.

use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::{AsModelResultMapped as _, DatabaseFetchError, ToValue as _},
};
use moosicbox_music_models::{
    ApiSource, AudioFormat, PlaybackQuality, TrackApiSource, TrackSize, id::Id,
};
use switchy_database::{
    DatabaseError, DatabaseValue, Row, boxed,
    profiles::LibraryDatabase,
    query::{
        FilterableQuery, SortDirection, coalesce, identifier, literal, select, where_in,
        where_not_eq,
    },
};
use thiserror::Error;

/// Database model types for library entities.
pub mod models;

use crate::{
    db::models::LibraryConfig,
    models::{LibraryAlbum, LibraryArtist, LibraryTrack},
};

/// Creates or updates a library authentication configuration.
///
/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub async fn create_library_config(
    db: &LibraryDatabase,
    client_id: &str,
    access_token: &str,
    refresh_token: &str,
    client_name: &str,
    expires_in: u32,
    scope: &str,
    token_type: &str,
    user: &str,
    user_id: u32,
) -> Result<(), DatabaseError> {
    db.upsert("library_config")
        .value("client_id", client_id)
        .value("access_token", access_token)
        .value("refresh_token", refresh_token)
        .value("client_name", client_name)
        .value("expires_in", expires_in)
        .value("scope", scope)
        .value("token_type", token_type)
        .value("user", user)
        .value("user_id", user_id)
        .where_eq("refresh_token", refresh_token)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Deletes a library authentication configuration by refresh token.
///
/// # Errors
///
/// * If there was a database error
pub async fn delete_library_config(
    db: &LibraryDatabase,
    refresh_token: &str,
) -> Result<(), DatabaseError> {
    db.delete("library_config")
        .where_eq("refresh_token", refresh_token)
        .execute(&**db)
        .await?;

    Ok(())
}

/// Errors that can occur when retrieving library configuration.
#[derive(Debug, Error)]
pub enum LibraryConfigError {
    /// Database operation failed.
    #[error(transparent)]
    Database(#[from] DatabaseError),
    /// Failed to parse configuration data.
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
    /// No library configurations are available.
    #[error("No configs available")]
    NoConfigsAvailable,
}

/// Retrieves the library authentication configuration.
///
/// # Errors
///
/// * If there was a database error
/// * If there were no configs available
pub async fn get_library_config(
    db: &LibraryDatabase,
) -> Result<Option<LibraryConfig>, LibraryConfigError> {
    let mut configs = db
        .select("library_config")
        .execute(&**db)
        .await?
        .to_value_type()?;

    if configs.is_empty() {
        return Err(LibraryConfigError::NoConfigsAvailable);
    }

    configs.sort_by(|a: &LibraryConfig, b: &LibraryConfig| a.issued_at.cmp(&b.issued_at));

    Ok(configs.first().cloned())
}

/// Retrieves the library access token and refresh token pair.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_library_access_tokens(
    db: &LibraryDatabase,
) -> Result<Option<(String, String)>, LibraryConfigError> {
    Ok(get_library_config(db)
        .await?
        .map(|c| (c.access_token.clone(), c.refresh_token)))
}

/// Retrieves the library access token.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_library_access_token(
    db: &LibraryDatabase,
) -> Result<Option<String>, LibraryConfigError> {
    Ok(get_library_access_tokens(db).await?.map(|c| c.0))
}

/// Retrieves all artists from the library database.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_artists(db: &LibraryDatabase) -> Result<Vec<LibraryArtist>, DatabaseFetchError> {
    Ok(db.select("artists").execute(&**db).await?.to_value_type()?)
}

/// Retrieves all albums from the library database with audio format information.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_albums(db: &LibraryDatabase) -> Result<Vec<LibraryAlbum>, DatabaseFetchError> {
    db.select("albums")
        .distinct()
        .columns(&[
            "albums.*",
            "albums.id as album_id",
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "track_sizes.format",
            "artists.title as artist",
            "tracks.source",
            "artists.api_sources as artist_api_sources",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .join("artists", "artists.id=albums.artist_id")
        .sort("albums.id", SortDirection::Desc)
        .where_or(boxed![
            where_not_eq("track_sizes.format", AudioFormat::Source.as_ref()),
            where_not_eq("tracks.source", TrackApiSource::Local.to_string())
        ])
        .execute(&**db)
        .await?
        .as_model_mapped()
}

/// Retrieves a specific artist by ID from a given API source.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_artist(
    db: &LibraryDatabase,
    api_source: &ApiSource,
    id: &Id,
) -> Result<Option<LibraryArtist>, DatabaseFetchError> {
    Ok(if api_source.is_library() {
        db.select("artists")
            .where_eq("id", id)
            .execute_first(&**db)
            .await?
            .as_ref()
            .to_value_type()?
    } else {
        db.select("artists")
            .join(
                "api_sources",
                "api_sources.entity_type='artists' AND api_sources.entity_id = artists.id",
            )
            .where_eq("api_sources.source", api_source.as_ref())
            .where_eq("api_sources.source_id", id)
            .execute_first(&**db)
            .await?
            .as_ref()
            .to_value_type()?
    })
}

/// Retrieves the artist associated with a specific album ID.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_artist_by_album_id(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<LibraryArtist>, DatabaseFetchError> {
    Ok(db
        .select("artists")
        .where_eq("albums.id", id)
        .join("albums", "albums.artist_id = artists.id")
        .execute_first(&**db)
        .await?
        .as_ref()
        .to_value_type()?)
}

/// Retrieves all artists associated with the given album IDs.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_artists_by_album_ids(
    db: &LibraryDatabase,
    album_ids: &[u64],
) -> Result<Vec<LibraryArtist>, DatabaseFetchError> {
    Ok(db
        .select("artists")
        .distinct()
        .join("albums", "albums.artist_id = artists.id")
        .where_in("album.id", album_ids.to_vec())
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Retrieves the artist for a specific album.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_album_artist(
    db: &LibraryDatabase,
    album_id: u64,
) -> Result<Option<LibraryArtist>, DatabaseFetchError> {
    Ok(db
        .select("artists")
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.id", album_id)
        .execute_first(&**db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

/// Retrieves a specific album by ID from a given API source.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_album(
    db: &LibraryDatabase,
    api_source: &ApiSource,
    id: &Id,
) -> Result<Option<LibraryAlbum>, DatabaseFetchError> {
    Ok(if api_source.is_library() {
        db.select("albums")
            .columns(&[
                "albums.*",
                "artists.title as artist",
                "artists.api_sources as artist_api_sources",
            ])
            .join("artists", "artists.id = albums.artist_id")
            .where_eq("albums.id", id)
            .execute_first(&**db)
            .await?
            .as_ref()
            .to_value_type()?
    } else {
        db.select("albums")
            .columns(&[
                "albums.*",
                "artists.title as artist",
                "artists.api_sources as artist_api_sources",
            ])
            .join("artists", "artists.id = albums.artist_id")
            .join(
                "api_sources",
                "api_sources.entity_type='albums' AND api_sources.entity_id = albums.id",
            )
            .where_eq("api_sources.source", api_source.as_ref())
            .where_eq("api_sources.source_id", id)
            .execute_first(&**db)
            .await?
            .as_ref()
            .to_value_type()?
    })
}

/// Retrieves all tracks for a specific album.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_album_tracks(
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<Vec<LibraryTrack>, DatabaseFetchError> {
    Ok(db
        .select("tracks")
        .columns(&[
            "tracks.*",
            "albums.title as album",
            "albums.blur as blur",
            "albums.date_released as date_released",
            "albums.date_added as date_added",
            "artists.title as artist",
            "artists.id as artist_id",
            "albums.artwork",
            "track_sizes.format",
            "track_sizes.bytes",
            "track_sizes.bit_depth",
            "track_sizes.audio_bitrate",
            "track_sizes.overall_bitrate",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "albums.api_sources as album_api_sources",
            "artists.api_sources as artist_api_sources",
        ])
        .where_eq("tracks.album_id", album_id)
        .join("albums", "albums.id=tracks.album_id")
        .join("artists", "artists.id=albums.artist_id")
        .left_join(
            "track_sizes",
            "tracks.id=track_sizes.track_id AND track_sizes.format=tracks.format",
        )
        .sort("number", SortDirection::Asc)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Retrieves all albums for a specific artist.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_artist_albums(
    db: &LibraryDatabase,
    artist_id: &Id,
) -> Result<Vec<LibraryAlbum>, DatabaseFetchError> {
    db.select("albums")
        .distinct()
        .columns(&[
            "albums.*",
            "albums.id as album_id",
            "track_sizes.bit_depth",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "artists.title as artist",
            "tracks.format",
            "tracks.source",
            "artists.api_sources as artist_api_sources",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .join("artists", "artists.id=albums.artist_id")
        .where_eq("albums.artist_id", artist_id)
        .sort("albums.id", SortDirection::Desc)
        .execute(&**db)
        .await?
        .as_model_mapped()
}

/// Parameters for creating or updating track size information in the database.
///
/// Contains all the audio quality metadata for a track at a specific playback quality level.
#[derive(Debug, Clone)]
pub struct SetTrackSize {
    /// Track ID.
    pub track_id: u64,
    /// Playback quality.
    pub quality: PlaybackQuality,
    /// File size in bytes.
    pub bytes: Option<Option<u64>>,
    /// Audio bit depth.
    pub bit_depth: Option<Option<u8>>,
    /// Audio bitrate.
    pub audio_bitrate: Option<Option<u32>>,
    /// Overall bitrate.
    pub overall_bitrate: Option<Option<u32>>,
    /// Sample rate.
    pub sample_rate: Option<Option<u32>>,
    /// Number of audio channels.
    pub channels: Option<Option<u8>>,
}

/// Sets the audio size information for a single track.
///
/// # Errors
///
/// * If there was a database error
pub async fn set_track_size(
    db: &LibraryDatabase,
    value: SetTrackSize,
) -> Result<Option<TrackSize>, DatabaseFetchError> {
    Ok(set_track_sizes(db, &[value]).await?.first().cloned())
}

/// Sets the audio size information for multiple tracks.
///
/// # Errors
///
/// * If there was a database error
pub async fn set_track_sizes(
    db: &LibraryDatabase,
    values: &[SetTrackSize],
) -> Result<Vec<TrackSize>, DatabaseFetchError> {
    let values = values
        .iter()
        .map(|v| {
            let mut values = vec![
                (
                    "track_id",
                    #[allow(clippy::cast_possible_wrap)]
                    DatabaseValue::Int64(v.track_id as i64),
                ),
                (
                    "format",
                    DatabaseValue::String(v.quality.format.as_ref().to_string()),
                ),
            ];

            if let Some(bytes) = v.bytes {
                values.push((
                    "bytes",
                    #[allow(clippy::cast_possible_wrap)]
                    DatabaseValue::Int64Opt(bytes.map(|x| x as i64)),
                ));
            }
            if let Some(bit_depth) = v.bit_depth {
                values.push((
                    "bit_depth",
                    DatabaseValue::Int64Opt(bit_depth.map(i64::from)),
                ));
            }
            if let Some(audio_bitrate) = v.audio_bitrate {
                values.push((
                    "audio_bitrate",
                    DatabaseValue::Int64Opt(audio_bitrate.map(i64::from)),
                ));
            }
            if let Some(overall_bitrate) = v.overall_bitrate {
                values.push((
                    "overall_bitrate",
                    DatabaseValue::Int64Opt(overall_bitrate.map(i64::from)),
                ));
            }
            if let Some(sample_rate) = v.sample_rate {
                values.push((
                    "sample_rate",
                    DatabaseValue::Int64Opt(sample_rate.map(i64::from)),
                ));
            }
            if let Some(channels) = v.channels {
                values.push(("channels", DatabaseValue::Int64Opt(channels.map(i64::from))));
            }

            values
        })
        .collect::<Vec<_>>();

    Ok(db
        .upsert_multi("track_sizes")
        .unique(boxed![
            identifier("track_id"),
            coalesce(boxed![identifier("format"), literal("''")]),
            coalesce(boxed![identifier("audio_bitrate"), literal("0")]),
            coalesce(boxed![identifier("overall_bitrate"), literal("0")]),
            coalesce(boxed![identifier("bit_depth"), literal("0")]),
            coalesce(boxed![identifier("sample_rate"), literal("0")]),
            coalesce(boxed![identifier("channels"), literal("0")]),
        ])
        .values(values.clone())
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Retrieves the file size in bytes for a track at a specific quality level.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_track_size(
    db: &LibraryDatabase,
    id: &Id,
    quality: &PlaybackQuality,
) -> Result<Option<u64>, DatabaseFetchError> {
    Ok(db
        .select("track_sizes")
        .columns(&["bytes"])
        .where_eq("track_id", id.to_string())
        .where_eq("format", quality.format.as_ref())
        .execute_first(&**db)
        .await?
        .and_then(|x| x.columns.first().cloned())
        .map(|(_, value)| value)
        .map(|col| col.to_value_type() as Result<Option<u64>, _>)
        .transpose()?
        .flatten())
}

/// Retrieves a specific track by ID.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_track(
    db: &LibraryDatabase,
    id: &Id,
) -> Result<Option<LibraryTrack>, DatabaseFetchError> {
    Ok(get_tracks(db, Some(&[id.to_owned()]))
        .await?
        .into_iter()
        .next())
}

/// Retrieves tracks by their IDs, or all tracks if no IDs are provided.
///
/// # Errors
///
/// * If there was a database error
pub async fn get_tracks(
    db: &LibraryDatabase,
    ids: Option<&[Id]>,
) -> Result<Vec<LibraryTrack>, DatabaseFetchError> {
    if ids.is_some_and(<[Id]>::is_empty) {
        return Ok(vec![]);
    }

    Ok(db
        .select("tracks")
        .columns(&[
            "tracks.*",
            "albums.title as album",
            "albums.blur as blur",
            "albums.date_released as date_released",
            "albums.date_added as date_added",
            "artists.title as artist",
            "artists.id as artist_id",
            "albums.artwork",
            "track_sizes.format",
            "track_sizes.bytes",
            "track_sizes.bit_depth",
            "track_sizes.audio_bitrate",
            "track_sizes.overall_bitrate",
            "track_sizes.sample_rate",
            "track_sizes.channels",
            "albums.api_sources as album_api_sources",
            "artists.api_sources as artist_api_sources",
        ])
        .filter_if_some(ids.map(|ids| where_in("tracks.id", ids.to_vec())))
        .join("albums", "albums.id=tracks.album_id")
        .join("artists", "artists.id=albums.artist_id")
        .left_join(
            "track_sizes",
            "tracks.id=track_sizes.track_id AND track_sizes.format=tracks.format",
        )
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Deletes a single track from the database by its ID.
///
/// # Errors
///
/// * If there was a database error
pub async fn delete_track(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<LibraryTrack>, DatabaseFetchError> {
    Ok(delete_tracks(db, Some(&vec![id])).await?.into_iter().next())
}

/// Deletes multiple tracks from the database by their IDs.
///
/// # Errors
///
/// * If there was a database error
pub async fn delete_tracks(
    db: &LibraryDatabase,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<LibraryTrack>, DatabaseFetchError> {
    if ids.is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("tracks")
        .filter_if_some(ids.map(|ids| where_in("id", ids.clone())))
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Deletes track size information for a single track.
///
/// # Errors
///
/// * If there was a database error
pub async fn delete_track_size_by_track_id(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<TrackSize>, DatabaseFetchError> {
    Ok(delete_track_sizes_by_track_id(db, Some(&vec![id]))
        .await?
        .into_iter()
        .next())
}

/// Deletes track size information for multiple tracks.
///
/// # Errors
///
/// * If there was a database error
pub async fn delete_track_sizes_by_track_id(
    db: &LibraryDatabase,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<TrackSize>, DatabaseFetchError> {
    if ids.is_some_and(Vec::is_empty) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("track_sizes")
        .filter_if_some(ids.map(|ids| where_in("track_id", ids.clone())))
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Adds a single artist to the database and returns the created/existing artist.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_artist_and_get_artist(
    db: &LibraryDatabase,
    artist: LibraryArtist,
) -> Result<LibraryArtist, DatabaseFetchError> {
    Ok(add_artists_and_get_artists(db, vec![artist]).await?[0].clone())
}

/// Adds a single artist from a field map to the database and returns the created/existing artist.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_artist_map_and_get_artist(
    db: &LibraryDatabase,
    artist: Vec<(&str, DatabaseValue)>,
) -> Result<LibraryArtist, DatabaseFetchError> {
    Ok(add_artist_maps_and_get_artists(db, vec![artist]).await?[0].clone())
}

/// Adds multiple artists to the database and returns the created/existing artists.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_artists_and_get_artists(
    db: &LibraryDatabase,
    artists: Vec<LibraryArtist>,
) -> Result<Vec<LibraryArtist>, DatabaseFetchError> {
    add_artist_maps_and_get_artists(
        db,
        artists
            .into_iter()
            .map(|artist| {
                vec![
                    ("title", DatabaseValue::String(artist.title)),
                    ("cover", DatabaseValue::StringOpt(artist.cover)),
                ]
            })
            .collect(),
    )
    .await
}

/// Adds multiple artists from field maps to the database and returns the created/existing artists.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_artist_maps_and_get_artists(
    db: &LibraryDatabase,
    artists: Vec<Vec<(&str, DatabaseValue)>>,
) -> Result<Vec<LibraryArtist>, DatabaseFetchError> {
    let mut results = vec![];

    for artist in artists {
        let title = artist
            .iter()
            .find(|(key, _)| *key == "title")
            .and_then(|(_, value)| value.as_str().map(ToString::to_string))
            .ok_or(DatabaseFetchError::InvalidRequest)?;

        let row: LibraryArtist = db
            .upsert("artists")
            .where_eq("title", title)
            .values(artist.into_iter().collect::<Vec<_>>())
            .execute_first(&**db)
            .await?
            .to_value_type()?;

        results.push(row);
    }

    Ok(results)
}

/// Adds multiple albums to the database without returning them.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_albums(
    db: &LibraryDatabase,
    albums: Vec<LibraryAlbum>,
) -> Result<Vec<LibraryAlbum>, DatabaseFetchError> {
    let mut data: Vec<LibraryAlbum> = Vec::new();

    for album in albums {
        data.push(
            db.upsert("albums")
                .where_eq("artist_id", album.artist_id)
                .where_eq("title", album.title.clone())
                .where_eq("directory", album.directory.clone())
                .value("artist_id", album.artist_id)
                .value("title", album.title)
                .value("directory", album.directory)
                .value("date_released", album.date_released)
                .value("artwork", album.artwork)
                .execute_first(&**db)
                .await?
                .to_value_type()?,
        );
    }

    Ok(data)
}

/// Adds a single album to the database and returns the created/existing album.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_album_and_get_album(
    db: &LibraryDatabase,
    album: LibraryAlbum,
) -> Result<LibraryAlbum, DatabaseFetchError> {
    Ok(add_albums_and_get_albums(db, vec![album]).await?[0].clone())
}

/// Adds a single album from a field map to the database and returns the created/existing album.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_album_map_and_get_album(
    db: &LibraryDatabase,
    album: Vec<(&str, DatabaseValue)>,
) -> Result<LibraryAlbum, DatabaseFetchError> {
    Ok(add_album_maps_and_get_albums(db, vec![album]).await?[0].clone())
}

/// Adds multiple albums to the database and returns the created/existing albums.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_albums_and_get_albums(
    db: &LibraryDatabase,
    albums: Vec<LibraryAlbum>,
) -> Result<Vec<LibraryAlbum>, DatabaseFetchError> {
    add_album_maps_and_get_albums(
        db,
        albums
            .into_iter()
            .map(|album| {
                vec![
                    (
                        "artist_id",
                        #[allow(clippy::cast_possible_wrap)]
                        DatabaseValue::Int64(album.artist_id as i64),
                    ),
                    ("title", DatabaseValue::String(album.title)),
                    (
                        "date_released",
                        DatabaseValue::StringOpt(album.date_released),
                    ),
                    ("artwork", DatabaseValue::StringOpt(album.artwork)),
                    ("directory", DatabaseValue::StringOpt(album.directory)),
                ]
            })
            .collect(),
    )
    .await
}

/// Adds multiple albums from field maps to the database and returns the created/existing albums.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_album_maps_and_get_albums(
    db: &LibraryDatabase,
    albums: Vec<Vec<(&str, DatabaseValue)>>,
) -> Result<Vec<LibraryAlbum>, DatabaseFetchError> {
    let mut values = vec![];

    for album in albums {
        if !album.iter().any(|(x, _)| *x == "artist_id")
            || !album.iter().any(|(x, _)| *x == "title")
        {
            return Err(DatabaseFetchError::InvalidRequest);
        }

        let mut album_values = album.into_iter().collect::<Vec<_>>();
        album_values.sort_by(|a, b| a.0.cmp(b.0));
        values.push(album_values);
    }

    Ok(db
        .upsert_multi("albums")
        .unique(boxed![identifier("artist_id"), identifier("title")])
        .values(values)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Parameters for inserting a new track into the database.
///
/// Contains the track data along with album association and optional file path.
#[derive(Debug, Clone, Default)]
pub struct InsertTrack {
    /// Track to insert.
    pub track: LibraryTrack,
    /// Album ID.
    pub album_id: u64,
    /// File path.
    pub file: Option<String>,
}

/// Adds multiple tracks to the database and returns the created/existing tracks.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_tracks(
    db: &LibraryDatabase,
    tracks: Vec<InsertTrack>,
) -> Result<Vec<LibraryTrack>, DatabaseFetchError> {
    let values = tracks
        .iter()
        .map(|insert| {
            let mut values = vec![
                (
                    "number",
                    DatabaseValue::Int64(i64::from(insert.track.number)),
                ),
                ("duration", DatabaseValue::Real64(insert.track.duration)),
                (
                    "album_id",
                    #[allow(clippy::cast_possible_wrap)]
                    DatabaseValue::Int64(insert.album_id as i64),
                ),
                ("title", DatabaseValue::String(insert.track.title.clone())),
                (
                    "format",
                    DatabaseValue::String(
                        insert.track.format.unwrap_or_default().as_ref().to_string(),
                    ),
                ),
                (
                    "source",
                    DatabaseValue::String(insert.track.source.to_string()),
                ),
            ];

            if let Some(file) = &insert.file {
                values.push(("file", DatabaseValue::String(file.clone())));
            }

            values
        })
        .collect::<Vec<_>>();

    Ok(db
        .upsert_multi("tracks")
        .unique(boxed![
            coalesce(boxed![identifier("file"), literal("''")]),
            identifier("album_id"),
            identifier("title"),
            identifier("duration"),
            identifier("number"),
            coalesce(boxed![identifier("format"), literal("''")]),
            identifier("source"),
        ])
        .values(values)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Parameters for inserting an API source mapping into the database.
///
/// Links a library entity (artist, album, or track) to an external API source.
#[derive(Debug, Clone, Default)]
pub struct InsertApiSource {
    /// Entity type.
    pub entity_type: String,
    /// Entity ID.
    pub entity_id: u64,
    /// API source name.
    pub source: String,
    /// API source ID.
    pub source_id: String,
}

/// API source mapping between library entities and external API sources.
///
/// Represents the relationship between a local library entity and its corresponding
/// identifier in an external music API.
pub struct ApiSourceMapping {
    /// Entity type (e.g., "artists", "albums", "tracks").
    pub entity_type: String,
    /// Entity ID.
    pub entity_id: u64,
    /// API source name.
    pub source: String,
    /// API source ID.
    pub source_id: String,
}

impl ToValueType<ApiSourceMapping> for &switchy_database::Row {
    fn to_value_type(self) -> Result<ApiSourceMapping, ParseError> {
        Ok(ApiSourceMapping {
            entity_type: self.to_value("entity_type")?,
            entity_id: self.to_value("entity_id")?,
            source: self.to_value("source")?,
            source_id: self.to_value("source_id")?,
        })
    }
}

/// Adds multiple API source mappings to the database.
///
/// # Errors
///
/// * If there was a database error
pub async fn add_api_sources(
    db: &LibraryDatabase,
    api_sources: Vec<InsertApiSource>,
) -> Result<Vec<ApiSourceMapping>, DatabaseFetchError> {
    let values = api_sources
        .iter()
        .map(|insert| {
            vec![
                (
                    "entity_type",
                    DatabaseValue::String(insert.entity_type.clone()),
                ),
                ("entity_id", DatabaseValue::UInt64(insert.entity_id)),
                ("source", DatabaseValue::String(insert.source.clone())),
                ("source_id", DatabaseValue::String(insert.source_id.clone())),
            ]
        })
        .collect::<Vec<_>>();

    Ok(db
        .upsert_multi("api_sources")
        .unique(boxed![
            identifier("entity_type"),
            identifier("entity_id"),
            identifier("source"),
        ])
        .values(values)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Parameters for updating an existing API source mapping.
///
/// Used to modify the external API source identifier for a library entity.
#[derive(Debug, Clone, Default)]
pub struct UpdateApiSource {
    /// Entity ID.
    pub entity_id: u64,
    /// API source name.
    pub source: String,
    /// API source ID.
    pub source_id: String,
}

/// Updates API source mappings for a table by regenerating the JSON array.
///
/// # Errors
///
/// * If there was a database error
pub async fn update_api_sources(
    db: &LibraryDatabase,
    table: &str,
) -> Result<Vec<Row>, DatabaseFetchError> {
    Ok(db
        .update(table)
        .value(
            "api_sources",
            Box::new(
                select("api_sources")
                    .columns(&["\
                    json_group_array(
                        json_object(
                           'id', api_sources.source_id,
                           'source', api_sources.source
                        )
                    )\
                    "])
                    .where_eq("api_sources.entity_type", table)
                    .where_eq("api_sources.entity_id", identifier(&format!("{table}.id"))),
            ) as Box<dyn switchy_database::query::Expression>,
        )
        .execute(&**db)
        .await?)
}
