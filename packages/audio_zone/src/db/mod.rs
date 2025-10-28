//! Database operations for audio zone management.
//!
//! This module provides low-level database functions for managing audio zones, including
//! CRUD operations, player associations, and conversions between database models and domain models.

#![allow(clippy::module_name_repetitions)]

use std::sync::Arc;

use models::AudioZoneWithSessionModel;
use moosicbox_audio_zone_models::{AudioZone, AudioZoneWithSession};
use moosicbox_json_utils::{ToValueType, database::DatabaseFetchError};
use switchy_database::{
    Database, DatabaseValue, boxed,
    config::ConfigDatabase,
    profiles::LibraryDatabase,
    query::{FilterableQuery, identifier},
};

use crate::models::{CreateAudioZone, UpdateAudioZone};

use self::models::AudioZoneModel;

pub mod models;

/// Updates an existing audio zone in the database and manages its player associations.
///
/// This function updates the zone's properties and synchronizes the associated players by
/// adding new players and removing players that are no longer in the zone.
///
/// # Errors
///
/// * If there is a database error
pub async fn update_audio_zone(
    db: &ConfigDatabase,
    zone: UpdateAudioZone,
) -> Result<models::AudioZoneModel, DatabaseFetchError> {
    let inserted: models::AudioZoneModel = db
        .upsert("audio_zones")
        .where_eq("id", zone.id)
        .value_opt("name", zone.name)
        .execute_first(&**db)
        .await?
        .to_value_type()?;

    if let Some(players) = zone.players {
        let mut existing: Vec<models::AudioZonePlayer> = db
            .select("audio_zone_players")
            .where_eq("audio_zone_id", inserted.id)
            .execute(&**db)
            .await?
            .to_value_type()?;

        existing.retain(|p| players.contains(&p.player_id));

        db.delete("audio_zone_players")
            .where_eq("audio_zone_id", inserted.id)
            .where_not_in(
                "player_id",
                existing.iter().map(|x| x.player_id).collect::<Vec<_>>(),
            )
            .execute(&**db)
            .await?;

        let values = players
            .into_iter()
            .filter(|x| !existing.iter().any(|existing| existing.player_id == *x))
            .map(|x| {
                vec![
                    ("audio_zone_id", DatabaseValue::UInt64(inserted.id)),
                    ("player_id", DatabaseValue::UInt64(x)),
                ]
            })
            .collect::<Vec<_>>();

        db.upsert_multi("audio_zone_players")
            .unique(boxed![identifier("audio_zone_id"), identifier("player_id"),])
            .values(values.clone())
            .execute(&**db)
            .await?;
    }

    Ok(inserted)
}

/// Retrieves all audio zones from the database as raw database models.
///
/// # Errors
///
/// * If there is a database error
pub async fn get_zones(
    db: &ConfigDatabase,
) -> Result<Vec<models::AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .select("audio_zones")
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Retrieves all audio zones with their associated playback sessions.
///
/// This function joins data from the config database (zones) and library database (sessions)
/// to provide zones that currently have active playback sessions.
///
/// # Errors
///
/// * If there is a database error
pub async fn get_zone_with_sessions(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
) -> Result<Vec<models::AudioZoneWithSessionModel>, DatabaseFetchError> {
    let zones: Vec<models::AudioZoneModel> = config_db
        .select("audio_zones")
        .columns(&["audio_zones.*"])
        .execute(&**config_db)
        .await?
        .to_value_type()?;

    let sessions: Vec<models::AudioZoneIdWithSessionIdModel> = library_db
        .select("sessions")
        .columns(&["id as session_id", "audio_zone_id"])
        .where_in(
            "audio_zone_id",
            zones.iter().map(|x| x.id).collect::<Vec<_>>(),
        )
        .execute(&**library_db)
        .await?
        .to_value_type()?;

    Ok(sessions
        .into_iter()
        .filter_map(|x| {
            zones.iter().find(|z| z.id == x.audio_zone_id).map(|zone| {
                models::AudioZoneWithSessionModel {
                    id: zone.id,
                    session_id: x.session_id,
                    name: zone.name.clone(),
                }
            })
        })
        .collect())
}

/// Creates a new audio zone in the database.
///
/// # Errors
///
/// * If there is a database error
pub async fn create_audio_zone(
    db: &ConfigDatabase,
    zone: &CreateAudioZone,
) -> Result<AudioZoneModel, DatabaseFetchError> {
    Ok(db
        .insert("audio_zones")
        .value("name", zone.name.clone())
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Deletes an audio zone from the database by its ID.
///
/// Returns the deleted zone if it existed, or `None` if no zone with the given ID was found.
///
/// # Errors
///
/// * If there is a database error
pub async fn delete_audio_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .delete("audio_zones")
        .where_eq("id", id)
        .execute_first(&**db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

/// Retrieves a specific audio zone from the database by its ID.
///
/// Returns `None` if no audio zone exists with the given ID.
///
/// # Errors
///
/// * If there is a database error
pub async fn get_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<models::AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .select("audio_zones")
        .where_eq("id", id)
        .execute_first(&**db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

/// Retrieves all players associated with a specific audio zone.
///
/// # Errors
///
/// * If there is a database error
pub async fn get_players(
    db: &ConfigDatabase,
    audio_zone_id: u64,
) -> Result<Vec<crate::models::Player>, DatabaseFetchError> {
    Ok(db
        .select("players")
        .columns(&["players.*"])
        .join(
            "audio_zone_players",
            "audio_zone_players.player_id=players.id",
        )
        .where_eq("audio_zone_players.audio_zone_id", audio_zone_id)
        .execute(&**db)
        .await?
        .to_value_type()?)
}

/// Converts a database audio zone model into the domain `AudioZone` type.
///
/// This function fetches the associated players for the zone and constructs a complete
/// `AudioZone` object.
///
/// # Errors
///
/// * If there is a database error
pub async fn audio_zone_try_from_db(
    value: AudioZoneModel,
    db: Arc<Box<dyn Database>>,
) -> Result<AudioZone, DatabaseFetchError> {
    Ok(AudioZone {
        id: value.id,
        name: value.name,
        players: crate::db::get_players(&db.into(), value.id).await?,
    })
}

/// Converts a database audio zone with session model into the domain `AudioZoneWithSession` type.
///
/// This function fetches the associated players for the zone and constructs a complete
/// `AudioZoneWithSession` object.
///
/// # Errors
///
/// * If there is a database error
pub async fn audio_zone_with_session_try_from_db(
    value: AudioZoneWithSessionModel,
    db: Arc<Box<dyn Database>>,
) -> Result<AudioZoneWithSession, DatabaseFetchError> {
    Ok(AudioZoneWithSession {
        id: value.id,
        session_id: value.session_id,
        name: value.name,
        players: crate::db::get_players(&db.into(), value.id).await?,
    })
}
