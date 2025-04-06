use std::collections::HashMap;

use moosicbox_database::{
    DatabaseError, DatabaseValue, boxed,
    profiles::LibraryDatabase,
    query::{
        FilterableQuery, SortDirection, coalesce, identifier, literal, where_in, where_not_eq,
    },
};
use moosicbox_json_utils::{
    ToValueType,
    database::{AsModelResultMapped as _, DatabaseFetchError},
};
use moosicbox_music_models::{AudioFormat, PlaybackQuality, TrackApiSource, TrackSize, id::Id};
use thiserror::Error;

pub mod models;

use crate::{
    db::models::LibraryConfig,
    models::{LibraryAlbum, LibraryArtist, LibraryTrack},
};

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

#[derive(Debug, Error)]
pub enum LibraryConfigError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
    #[error("No configs available")]
    NoConfigsAvailable,
}

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

/// # Errors
///
/// * If there was a database error
pub async fn get_library_access_token(
    db: &LibraryDatabase,
) -> Result<Option<String>, LibraryConfigError> {
    Ok(get_library_access_tokens(db).await?.map(|c| c.0))
}

/// # Errors
///
/// * If there was a database error
pub async fn get_artists(db: &LibraryDatabase) -> Result<Vec<LibraryArtist>, DatabaseFetchError> {
    Ok(db.select("artists").execute(&**db).await?.to_value_type()?)
}

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
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "tracks.source",
        ])
        .left_join("tracks", "tracks.album_id=albums.id")
        .left_join("track_sizes", "track_sizes.track_id=tracks.id")
        .join("artists", "artists.id=albums.artist_id")
        .sort("albums.id", SortDirection::Desc)
        .where_or(boxed![
            where_not_eq("track_sizes.format", AudioFormat::Source.as_ref()),
            where_not_eq("tracks.source", TrackApiSource::Local.as_ref())
        ])
        .execute(&**db)
        .await?
        .as_model_mapped()
}

/// # Errors
///
/// * If there was a database error
pub async fn get_artist(
    db: &LibraryDatabase,
    column: &str,
    id: &Id,
) -> Result<Option<LibraryArtist>, DatabaseFetchError> {
    Ok(db
        .select("artists")
        .where_eq(column.to_string(), id)
        .execute_first(&**db)
        .await?
        .as_ref()
        .to_value_type()?)
}

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

/// # Errors
///
/// * If there was a database error
pub async fn get_album_artist(
    db: &LibraryDatabase,
    album_id: u64,
) -> Result<Option<LibraryArtist>, DatabaseFetchError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.id", album_id)
        .execute_first(&**db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

/// # Errors
///
/// * If there was a database error
pub async fn get_tidal_album_artist(
    db: &LibraryDatabase,
    tidal_album_id: u64,
) -> Result<Option<LibraryArtist>, DatabaseFetchError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.tidal_id", tidal_album_id)
        .execute_first(&**db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

/// # Errors
///
/// * If there was a database error
pub async fn get_qobuz_album_artist(
    db: &LibraryDatabase,
    qobuz_album_id: &str,
) -> Result<Option<LibraryArtist>, DatabaseFetchError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.qobuz_id", qobuz_album_id)
        .execute_first(&**db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

/// # Errors
///
/// * If there was a database error
pub async fn get_album(
    db: &LibraryDatabase,
    column: &str,
    id: &Id,
) -> Result<Option<LibraryAlbum>, DatabaseFetchError> {
    Ok(db
        .select("albums")
        .columns(&[
            "albums.*",
            "artists.title as artist",
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
        ])
        .where_eq(format!("albums.{column}"), id)
        .join("artists", "artists.id = albums.artist_id")
        .execute_first(&**db)
        .await?
        .as_ref()
        .to_value_type()?)
}

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
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "artists.id as artist_id",
            "albums.artwork",
            "track_sizes.format",
            "track_sizes.bytes",
            "track_sizes.bit_depth",
            "track_sizes.audio_bitrate",
            "track_sizes.overall_bitrate",
            "track_sizes.sample_rate",
            "track_sizes.channels",
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
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "tracks.format",
            "tracks.source",
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

#[derive(Debug, Clone)]
pub struct SetTrackSize {
    pub track_id: u64,
    pub quality: PlaybackQuality,
    pub bytes: Option<Option<u64>>,
    pub bit_depth: Option<Option<u8>>,
    pub audio_bitrate: Option<Option<u32>>,
    pub overall_bitrate: Option<Option<u32>>,
    pub sample_rate: Option<Option<u32>>,
    pub channels: Option<Option<u8>>,
}

/// # Errors
///
/// * If there was a database error
pub async fn set_track_size(
    db: &LibraryDatabase,
    value: SetTrackSize,
) -> Result<Option<TrackSize>, DatabaseFetchError> {
    Ok(set_track_sizes(db, &[value]).await?.first().cloned())
}

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
                    DatabaseValue::Number(v.track_id as i64),
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
                    DatabaseValue::NumberOpt(bytes.map(|x| x as i64)),
                ));
            }
            if let Some(bit_depth) = v.bit_depth {
                values.push((
                    "bit_depth",
                    DatabaseValue::NumberOpt(bit_depth.map(i64::from)),
                ));
            }
            if let Some(audio_bitrate) = v.audio_bitrate {
                values.push((
                    "audio_bitrate",
                    DatabaseValue::NumberOpt(audio_bitrate.map(i64::from)),
                ));
            }
            if let Some(overall_bitrate) = v.overall_bitrate {
                values.push((
                    "overall_bitrate",
                    DatabaseValue::NumberOpt(overall_bitrate.map(i64::from)),
                ));
            }
            if let Some(sample_rate) = v.sample_rate {
                values.push((
                    "sample_rate",
                    DatabaseValue::NumberOpt(sample_rate.map(i64::from)),
                ));
            }
            if let Some(channels) = v.channels {
                values.push((
                    "channels",
                    DatabaseValue::NumberOpt(channels.map(i64::from)),
                ));
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
            "artists.tidal_id as tidal_artist_id",
            "artists.qobuz_id as qobuz_artist_id",
            "artists.id as artist_id",
            "albums.artwork",
            "track_sizes.format",
            "track_sizes.bytes",
            "track_sizes.bit_depth",
            "track_sizes.audio_bitrate",
            "track_sizes.overall_bitrate",
            "track_sizes.sample_rate",
            "track_sizes.channels",
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

/// # Errors
///
/// * If there was a database error
pub async fn delete_track(
    db: &LibraryDatabase,
    id: u64,
) -> Result<Option<LibraryTrack>, DatabaseFetchError> {
    Ok(delete_tracks(db, Some(&vec![id])).await?.into_iter().next())
}

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

/// # Errors
///
/// * If there was a database error
pub async fn add_artist_and_get_artist(
    db: &LibraryDatabase,
    artist: LibraryArtist,
) -> Result<LibraryArtist, DatabaseFetchError> {
    Ok(add_artists_and_get_artists(db, vec![artist]).await?[0].clone())
}

/// # Errors
///
/// * If there was a database error
pub async fn add_artist_map_and_get_artist<S: ::std::hash::BuildHasher + Send>(
    db: &LibraryDatabase,
    artist: HashMap<&str, DatabaseValue, S>,
) -> Result<LibraryArtist, DatabaseFetchError> {
    Ok(add_artist_maps_and_get_artists(db, vec![artist]).await?[0].clone())
}

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
                HashMap::from([
                    ("title", DatabaseValue::String(artist.title)),
                    ("cover", DatabaseValue::StringOpt(artist.cover)),
                ])
            })
            .collect(),
    )
    .await
}

/// # Errors
///
/// * If there was a database error
pub async fn add_artist_maps_and_get_artists<S: ::std::hash::BuildHasher + Send>(
    db: &LibraryDatabase,
    artists: Vec<HashMap<&str, DatabaseValue, S>>,
) -> Result<Vec<LibraryArtist>, DatabaseFetchError> {
    let mut results = vec![];

    for artist in artists {
        let Some(title) = artist.get("title") else {
            return Err(DatabaseFetchError::InvalidRequest);
        };

        let row: LibraryArtist = db
            .upsert("artists")
            .where_eq("title", title.clone())
            .values(artist.into_iter().collect::<Vec<_>>())
            .execute_first(&**db)
            .await?
            .to_value_type()?;

        results.push(row);
    }

    Ok(results)
}

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

/// # Errors
///
/// * If there was a database error
pub async fn add_album_and_get_album(
    db: &LibraryDatabase,
    album: LibraryAlbum,
) -> Result<LibraryAlbum, DatabaseFetchError> {
    Ok(add_albums_and_get_albums(db, vec![album]).await?[0].clone())
}

/// # Errors
///
/// * If there was a database error
pub async fn add_album_map_and_get_album<S: ::std::hash::BuildHasher + Send>(
    db: &LibraryDatabase,
    album: HashMap<&str, DatabaseValue, S>,
) -> Result<LibraryAlbum, DatabaseFetchError> {
    Ok(add_album_maps_and_get_albums(db, vec![album]).await?[0].clone())
}

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
                HashMap::from([
                    (
                        "artist_id",
                        #[allow(clippy::cast_possible_wrap)]
                        DatabaseValue::Number(album.artist_id as i64),
                    ),
                    ("title", DatabaseValue::String(album.title)),
                    (
                        "date_released",
                        DatabaseValue::StringOpt(album.date_released),
                    ),
                    ("artwork", DatabaseValue::StringOpt(album.artwork)),
                    ("directory", DatabaseValue::StringOpt(album.directory)),
                ])
            })
            .collect(),
    )
    .await
}

/// # Errors
///
/// * If there was a database error
pub async fn add_album_maps_and_get_albums<S: ::std::hash::BuildHasher + Send>(
    db: &LibraryDatabase,
    albums: Vec<HashMap<&str, DatabaseValue, S>>,
) -> Result<Vec<LibraryAlbum>, DatabaseFetchError> {
    let mut values = vec![];

    for album in albums {
        if !album.contains_key("artist_id") || !album.contains_key("title") {
            return Err(DatabaseFetchError::InvalidRequest);
        }

        let mut album_values = album.into_iter().collect::<Vec<_>>();
        album_values.sort_by(|a, b| a.0.cmp(b.0));
        values.push(album_values);
    }

    Ok(db
        .upsert_multi("albums")
        .unique(boxed![identifier("artist_id"), identifier("title"),])
        .values(values)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

#[derive(Debug, Clone, Default)]
pub struct InsertTrack {
    pub track: LibraryTrack,
    pub album_id: u64,
    pub file: Option<String>,
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
}

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
                    DatabaseValue::Number(i64::from(insert.track.number)),
                ),
                ("duration", DatabaseValue::Real(insert.track.duration)),
                (
                    "album_id",
                    #[allow(clippy::cast_possible_wrap)]
                    DatabaseValue::Number(insert.album_id as i64),
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
                    DatabaseValue::String(insert.track.source.as_ref().to_string()),
                ),
            ];

            if let Some(file) = &insert.file {
                values.push(("file", DatabaseValue::String(file.clone())));
            }

            if let Some(qobuz_id) = &insert.qobuz_id {
                values.push((
                    "qobuz_id",
                    #[allow(clippy::cast_possible_wrap)]
                    DatabaseValue::Number(*qobuz_id as i64),
                ));
            }

            if let Some(tidal_id) = &insert.tidal_id {
                values.push((
                    "tidal_id",
                    #[allow(clippy::cast_possible_wrap)]
                    DatabaseValue::Number(*tidal_id as i64),
                ));
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
            coalesce(boxed![identifier("tidal_id"), literal("0")]),
            coalesce(boxed![identifier("qobuz_id"), literal("0")]),
        ])
        .values(values)
        .execute(&**db)
        .await?
        .to_value_type()?)
}
