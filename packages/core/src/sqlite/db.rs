use moosicbox_database::{
    boxed, config::ConfigDatabase, profiles::LibraryDatabase, query::*, DatabaseError,
    DatabaseValue,
};
use moosicbox_json_utils::{database::DatabaseFetchError, ParseError, ToValueType as _};
use std::{fmt::Debug, sync::PoisonError};
use thiserror::Error;

use crate::types::AudioFormat;

use super::models::{AlbumVersionQuality, TrackApiSource};

impl<T> From<PoisonError<T>> for DbError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("No row")]
    NoRow,
    #[error("Invalid Request")]
    InvalidRequest,
    #[error("Poison Error")]
    PoisonError,
    #[error("Unknown DbError")]
    Unknown,
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

pub async fn get_client_id(db: &ConfigDatabase) -> Result<Option<String>, DbError> {
    Ok(db
        .select("client_access_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .sort("updated", SortDirection::Desc)
        .execute_first(db)
        .await?
        .and_then(|row| row.get("client_id"))
        .map(|value| value.to_value_type())
        .transpose()?)
}

pub async fn get_client_access_token(
    db: &ConfigDatabase,
) -> Result<Option<(String, String)>, DbError> {
    Ok(db
        .select("client_access_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .sort("updated", SortDirection::Desc)
        .execute_first(db)
        .await?
        .and_then(|row| {
            if let (Some(a), Some(b)) = (row.get("client_id"), row.get("token")) {
                Some((a, b))
            } else {
                None
            }
        })
        .map(|(client_id, token)| {
            Ok::<_, ParseError>((client_id.to_value_type()?, token.to_value_type()?))
        })
        .transpose()?)
}

pub async fn create_client_access_token(
    db: &ConfigDatabase,
    client_id: &str,
    token: &str,
) -> Result<(), DbError> {
    db.upsert("client_access_tokens")
        .where_eq("token", token)
        .where_eq("client_id", client_id)
        .value("token", token)
        .value("client_id", client_id)
        .execute_first(db)
        .await?;

    Ok(())
}

pub async fn delete_magic_token(db: &ConfigDatabase, magic_token: &str) -> Result<(), DbError> {
    db.delete("magic_tokens")
        .where_eq("magic_token", magic_token)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_credentials_from_magic_token(
    db: &ConfigDatabase,
    magic_token: &str,
) -> Result<Option<(String, String)>, DbError> {
    if let Some((client_id, access_token)) = db
        .select("magic_tokens")
        .where_or(boxed![
            where_eq("expires", DatabaseValue::Null),
            where_gt("expires", DatabaseValue::Now),
        ])
        .where_eq("magic_token", magic_token)
        .execute_first(db)
        .await?
        .and_then(|row| {
            if let (Some(a), Some(b)) = (row.get("client_id"), row.get("access_token")) {
                Some((a, b))
            } else {
                None
            }
        })
        .map(|(client_id, token)| {
            Ok::<_, ParseError>((client_id.to_value_type()?, token.to_value_type()?))
        })
        .transpose()?
    {
        delete_magic_token(db, magic_token).await?;

        Ok(Some((client_id, access_token)))
    } else {
        Ok(None)
    }
}

pub async fn save_magic_token(
    db: &ConfigDatabase,
    magic_token: &str,
    client_id: &str,
    access_token: &str,
) -> Result<(), DbError> {
    db.upsert("magic_tokens")
        .where_eq("magic_token", magic_token)
        .where_eq("access_token", access_token)
        .where_eq("client_id", client_id)
        .value("magic_token", magic_token)
        .value("access_token", access_token)
        .value("client_id", client_id)
        .value("expires", DatabaseValue::NowAdd("'+1 Day'".into()))
        .execute_first(db)
        .await?;

    Ok(())
}

pub async fn get_all_album_version_qualities(
    db: &LibraryDatabase,
    album_ids: Vec<u64>,
) -> Result<Vec<AlbumVersionQuality>, DbError> {
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
        .execute(db)
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

pub async fn get_album_version_qualities(
    db: &LibraryDatabase,
    album_id: u64,
) -> Result<Vec<AlbumVersionQuality>, DbError> {
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
        .execute(db)
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
