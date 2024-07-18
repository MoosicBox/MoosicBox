use std::collections::HashMap;

use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{AsModelResultMapped as _, Id, TrackApiSource, TrackSize},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_database::{boxed, query::*, Database, DatabaseError, DatabaseValue};
use moosicbox_json_utils::ToValueType;
use thiserror::Error;

pub mod models;

use crate::{
    db::models::LibraryConfig,
    models::{LibraryAlbum, LibraryArtist, LibraryTrack},
};

#[allow(clippy::too_many_arguments)]
pub async fn create_library_config(
    db: &dyn Database,
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
        .execute(db)
        .await?;

    Ok(())
}

pub async fn delete_library_config(
    db: &dyn Database,
    refresh_token: &str,
) -> Result<(), DatabaseError> {
    db.delete("library_config")
        .where_eq("refresh_token", refresh_token)
        .execute(db)
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

pub async fn get_library_config(
    db: &dyn Database,
) -> Result<Option<LibraryConfig>, LibraryConfigError> {
    let mut configs = db
        .select("library_config")
        .execute(db)
        .await?
        .to_value_type()?;

    if configs.is_empty() {
        return Err(LibraryConfigError::NoConfigsAvailable);
    }

    configs.sort_by(|a: &LibraryConfig, b: &LibraryConfig| a.issued_at.cmp(&b.issued_at));

    Ok(configs.first().cloned())
}

pub async fn get_library_access_tokens(
    db: &dyn Database,
) -> Result<Option<(String, String)>, LibraryConfigError> {
    Ok(get_library_config(db)
        .await?
        .map(|c| (c.access_token.clone(), c.refresh_token.clone())))
}

pub async fn get_library_access_token(
    db: &dyn Database,
) -> Result<Option<String>, LibraryConfigError> {
    Ok(get_library_access_tokens(db).await?.map(|c| c.0))
}

pub async fn get_artists(db: &dyn Database) -> Result<Vec<LibraryArtist>, DbError> {
    Ok(db.select("artists").execute(db).await?.to_value_type()?)
}

pub async fn get_albums(db: &dyn Database) -> Result<Vec<LibraryAlbum>, DbError> {
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
        .execute(db)
        .await?
        .as_model_mapped()
}

pub async fn get_artist(
    db: &dyn Database,
    column: &str,
    id: &Id,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .where_eq(column.to_string(), id)
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_artist_by_album_id(
    db: &dyn Database,
    id: u64,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .where_eq("albums.id", id)
        .join("albums", "albums.artist_id = artists.id")
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_artists_by_album_ids(
    db: &dyn Database,
    album_ids: &[i32],
) -> Result<Vec<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .distinct()
        .join("albums", "albums.artist_id = artists.id")
        .where_in("album.id", album_ids.to_vec())
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_album_artist(
    db: &dyn Database,
    album_id: i32,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.id", album_id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_tidal_album_artist(
    db: &dyn Database,
    tidal_album_id: i32,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.tidal_id", tidal_album_id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_qobuz_album_artist(
    db: &dyn Database,
    qobuz_album_id: i32,
) -> Result<Option<LibraryArtist>, DbError> {
    Ok(db
        .select("artists")
        .columns(&["artists.*"])
        .join("albums", "albums.artist_id=artists.id")
        .where_eq("albums.qobuz_id", qobuz_album_id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_album(
    db: &dyn Database,
    column: &str,
    id: &Id,
) -> Result<Option<LibraryAlbum>, DbError> {
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
        .execute_first(db)
        .await?
        .as_ref()
        .to_value_type()?)
}

pub async fn get_album_tracks(
    db: &dyn Database,
    album_id: &Id,
) -> Result<Vec<LibraryTrack>, DbError> {
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
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_artist_albums(
    db: &dyn Database,
    artist_id: &Id,
) -> Result<Vec<LibraryAlbum>, DbError> {
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
        .execute(db)
        .await?
        .as_model_mapped()
}

#[derive(Debug, Clone)]
pub struct SetTrackSize {
    pub track_id: i32,
    pub quality: PlaybackQuality,
    pub bytes: Option<Option<u64>>,
    pub bit_depth: Option<Option<u8>>,
    pub audio_bitrate: Option<Option<u32>>,
    pub overall_bitrate: Option<Option<u32>>,
    pub sample_rate: Option<Option<u32>>,
    pub channels: Option<Option<u8>>,
}

pub async fn set_track_size(db: &dyn Database, value: SetTrackSize) -> Result<TrackSize, DbError> {
    Ok(set_track_sizes(db, &[value])
        .await?
        .first()
        .ok_or(DbError::NoRow)?
        .clone())
}

pub async fn set_track_sizes(
    db: &dyn Database,
    values: &[SetTrackSize],
) -> Result<Vec<TrackSize>, DbError> {
    let values = values
        .iter()
        .map(|v| {
            let mut values = vec![
                ("track_id", DatabaseValue::Number(v.track_id as i64)),
                (
                    "format",
                    DatabaseValue::String(v.quality.format.as_ref().to_string()),
                ),
            ];

            if let Some(bytes) = v.bytes {
                values.push(("bytes", DatabaseValue::NumberOpt(bytes.map(|x| x as i64))));
            }
            if let Some(bit_depth) = v.bit_depth {
                values.push((
                    "bit_depth",
                    DatabaseValue::NumberOpt(bit_depth.map(|x| x as i64)),
                ));
            }
            if let Some(audio_bitrate) = v.audio_bitrate {
                values.push((
                    "audio_bitrate",
                    DatabaseValue::NumberOpt(audio_bitrate.map(|x| x as i64)),
                ));
            }
            if let Some(overall_bitrate) = v.overall_bitrate {
                values.push((
                    "overall_bitrate",
                    DatabaseValue::NumberOpt(overall_bitrate.map(|x| x as i64)),
                ));
            }
            if let Some(sample_rate) = v.sample_rate {
                values.push((
                    "sample_rate",
                    DatabaseValue::NumberOpt(sample_rate.map(|x| x as i64)),
                ));
            }
            if let Some(channels) = v.channels {
                values.push((
                    "channels",
                    DatabaseValue::NumberOpt(channels.map(|x| x as i64)),
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
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_track_size(
    db: &dyn Database,
    id: &Id,
    quality: &PlaybackQuality,
) -> Result<Option<u64>, DbError> {
    Ok(db
        .select("track_sizes")
        .columns(&["bytes"])
        .where_eq("track_id", id.to_string())
        .where_eq("format", quality.format.as_ref())
        .execute_first(db)
        .await?
        .and_then(|x| x.columns.first().cloned())
        .map(|(_, value)| value)
        .map(|col| col.to_value_type() as Result<Option<u64>, _>)
        .transpose()?
        .flatten())
}

pub async fn get_track(db: &dyn Database, id: &Id) -> Result<Option<LibraryTrack>, DbError> {
    Ok(get_tracks(db, Some(&[id.to_owned()]))
        .await?
        .into_iter()
        .next())
}

pub async fn get_tracks(
    db: &dyn Database,
    ids: Option<&[Id]>,
) -> Result<Vec<LibraryTrack>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
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
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn delete_track(db: &dyn Database, id: i32) -> Result<Option<LibraryTrack>, DbError> {
    Ok(delete_tracks(db, Some(&vec![id])).await?.into_iter().next())
}

pub async fn delete_tracks(
    db: &dyn Database,
    ids: Option<&Vec<i32>>,
) -> Result<Vec<LibraryTrack>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("tracks")
        .filter_if_some(ids.map(|ids| where_in("id", ids.to_vec())))
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn delete_track_size_by_track_id(
    db: &dyn Database,
    id: i32,
) -> Result<Option<TrackSize>, DbError> {
    Ok(delete_track_sizes_by_track_id(db, Some(&vec![id]))
        .await?
        .into_iter()
        .next())
}

pub async fn delete_track_sizes_by_track_id(
    db: &dyn Database,
    ids: Option<&Vec<i32>>,
) -> Result<Vec<TrackSize>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("track_sizes")
        .filter_if_some(ids.map(|ids| where_in("track_id", ids.to_vec())))
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn add_artist_and_get_artist(
    db: &dyn Database,
    artist: LibraryArtist,
) -> Result<LibraryArtist, DbError> {
    Ok(add_artists_and_get_artists(db, vec![artist]).await?[0].clone())
}

pub async fn add_artist_map_and_get_artist(
    db: &dyn Database,
    artist: HashMap<&str, DatabaseValue>,
) -> Result<LibraryArtist, DbError> {
    Ok(add_artist_maps_and_get_artists(db, vec![artist]).await?[0].clone())
}

pub async fn add_artists_and_get_artists(
    db: &dyn Database,
    artists: Vec<LibraryArtist>,
) -> Result<Vec<LibraryArtist>, DbError> {
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

pub async fn add_artist_maps_and_get_artists(
    db: &dyn Database,
    artists: Vec<HashMap<&str, DatabaseValue>>,
) -> Result<Vec<LibraryArtist>, DbError> {
    let mut results = vec![];

    for artist in artists {
        if !artist.contains_key("title") {
            return Err(DbError::InvalidRequest);
        }

        let row: LibraryArtist = db
            .upsert("artists")
            .where_eq("title", artist.get("title").unwrap().clone())
            .values(artist.into_iter().collect::<Vec<_>>())
            .execute_first(db)
            .await?
            .to_value_type()?;

        results.push(row);
    }

    Ok(results)
}

pub async fn add_albums(
    db: &dyn Database,
    albums: Vec<LibraryAlbum>,
) -> Result<Vec<LibraryAlbum>, DbError> {
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
                .execute_first(db)
                .await?
                .to_value_type()?,
        );
    }

    Ok(data)
}

pub async fn add_album_and_get_album(
    db: &dyn Database,
    album: LibraryAlbum,
) -> Result<LibraryAlbum, DbError> {
    Ok(add_albums_and_get_albums(db, vec![album]).await?[0].clone())
}

pub async fn add_album_map_and_get_album(
    db: &dyn Database,
    album: HashMap<&str, DatabaseValue>,
) -> Result<LibraryAlbum, DbError> {
    Ok(add_album_maps_and_get_albums(db, vec![album]).await?[0].clone())
}

pub async fn add_albums_and_get_albums(
    db: &dyn Database,
    albums: Vec<LibraryAlbum>,
) -> Result<Vec<LibraryAlbum>, DbError> {
    add_album_maps_and_get_albums(
        db,
        albums
            .into_iter()
            .map(|album| {
                HashMap::from([
                    ("artist_id", DatabaseValue::Number(album.artist_id as i64)),
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

pub async fn add_album_maps_and_get_albums(
    db: &dyn Database,
    albums: Vec<HashMap<&str, DatabaseValue>>,
) -> Result<Vec<LibraryAlbum>, DbError> {
    let mut values = vec![];

    for album in albums {
        if !album.contains_key("artist_id") || !album.contains_key("title") {
            return Err(DbError::InvalidRequest);
        }

        let mut album_values = album.into_iter().collect::<Vec<_>>();
        album_values.sort_by(|a, b| a.0.cmp(b.0));
        values.push(album_values);
    }

    Ok(db
        .upsert_multi("albums")
        .unique(boxed![identifier("artist_id"), identifier("title"),])
        .values(values)
        .execute(db)
        .await?
        .to_value_type()?)
}

#[derive(Debug, Clone, Default)]
pub struct InsertTrack {
    pub track: LibraryTrack,
    pub album_id: i32,
    pub file: Option<String>,
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
}

pub async fn add_tracks(
    db: &dyn Database,
    tracks: Vec<InsertTrack>,
) -> Result<Vec<LibraryTrack>, DbError> {
    let values = tracks
        .iter()
        .map(|insert| {
            let mut values = vec![
                ("number", DatabaseValue::Number(insert.track.number as i64)),
                ("duration", DatabaseValue::Real(insert.track.duration)),
                ("album_id", DatabaseValue::Number(insert.album_id as i64)),
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
                values.push(("qobuz_id", DatabaseValue::Number(*qobuz_id as i64)));
            }

            if let Some(tidal_id) = &insert.tidal_id {
                values.push(("tidal_id", DatabaseValue::Number(*tidal_id as i64)));
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
        .execute(db)
        .await?
        .to_value_type()?)
}
