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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_json_utils::database::ToValue as _;
    use serial_test::serial;
    use std::sync::Arc;
    use switchy_database::{Database, simulator::SimulationDatabase};

    /// Helper to create a test database with required schema
    async fn setup_test_db() -> ConfigDatabase {
        let db = SimulationDatabase::new().expect("Failed to create simulation database");

        // Create the audio_zones table
        db.exec_raw(
            "CREATE TABLE audio_zones (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL
            )",
        )
        .await
        .expect("Failed to create audio_zones table");

        // Create the players table (needed for joins)
        db.exec_raw(
            "CREATE TABLE players (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                audio_output_id TEXT NOT NULL,
                name TEXT NOT NULL,
                playing INTEGER NOT NULL DEFAULT 0,
                created TEXT NOT NULL DEFAULT '',
                updated TEXT NOT NULL DEFAULT ''
            )",
        )
        .await
        .expect("Failed to create players table");

        // Create the audio_zone_players join table
        db.exec_raw(
            "CREATE TABLE audio_zone_players (
                audio_zone_id INTEGER NOT NULL,
                player_id INTEGER NOT NULL,
                PRIMARY KEY (audio_zone_id, player_id),
                FOREIGN KEY (audio_zone_id) REFERENCES audio_zones(id),
                FOREIGN KEY (player_id) REFERENCES players(id)
            )",
        )
        .await
        .expect("Failed to create audio_zone_players table");

        ConfigDatabase {
            database: Arc::new(Box::new(db) as Box<dyn Database>),
        }
    }

    /// Helper to create a test library database with sessions table
    async fn setup_library_db() -> LibraryDatabase {
        let db = SimulationDatabase::new().expect("Failed to create simulation database");

        // Create the sessions table
        db.exec_raw(
            "CREATE TABLE sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                audio_zone_id INTEGER NOT NULL
            )",
        )
        .await
        .expect("Failed to create sessions table");

        LibraryDatabase {
            database: Arc::new(Box::new(db) as Box<dyn Database>),
        }
    }

    /// Helper to insert a player into the database and return its ID
    async fn insert_player(db: &ConfigDatabase, name: &str, audio_output_id: &str) -> u64 {
        let row = db
            .insert("players")
            .value("name", name)
            .value("audio_output_id", audio_output_id)
            .value("playing", 0_i64)
            .value("created", "")
            .value("updated", "")
            .execute(&**db)
            .await
            .expect("Failed to insert player");
        #[allow(clippy::cast_sign_loss)]
        let id = row.to_value::<i64>("id").expect("Failed to get id") as u64;
        id
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_create_audio_zone() {
        let db = setup_test_db().await;

        let create = CreateAudioZone {
            name: "Living Room".to_string(),
        };
        let zone = create_audio_zone(&db, &create)
            .await
            .expect("Failed to create zone");

        assert_eq!(zone.name, "Living Room");
        assert!(zone.id > 0);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_zones_empty() {
        let db = setup_test_db().await;

        let zones = get_zones(&db).await.expect("Failed to get zones");
        assert!(zones.is_empty());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_zones_multiple() {
        let db = setup_test_db().await;

        // Create multiple zones
        create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Zone 1".to_string(),
            },
        )
        .await
        .expect("Failed to create zone 1");
        create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Zone 2".to_string(),
            },
        )
        .await
        .expect("Failed to create zone 2");
        create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Zone 3".to_string(),
            },
        )
        .await
        .expect("Failed to create zone 3");

        let zones = get_zones(&db).await.expect("Failed to get zones");
        assert_eq!(zones.len(), 3);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_zone_by_id() {
        let db = setup_test_db().await;

        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Test Zone".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        let retrieved = get_zone(&db, created.id)
            .await
            .expect("Failed to get zone")
            .expect("Zone should exist");

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Test Zone");
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_zone_nonexistent() {
        let db = setup_test_db().await;

        let result = get_zone(&db, 999).await.expect("Failed to query zone");
        assert!(result.is_none());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_delete_audio_zone() {
        let db = setup_test_db().await;

        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "To Delete".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        let deleted = delete_audio_zone(&db, created.id)
            .await
            .expect("Failed to delete zone")
            .expect("Deleted zone should be returned");

        assert_eq!(deleted.id, created.id);
        assert_eq!(deleted.name, "To Delete");

        // Verify it's actually deleted
        let result = get_zone(&db, created.id)
            .await
            .expect("Failed to query zone");
        assert!(result.is_none());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_delete_nonexistent_zone() {
        let db = setup_test_db().await;

        let result = delete_audio_zone(&db, 999)
            .await
            .expect("Failed to delete zone");
        assert!(result.is_none());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_update_audio_zone_name_only() {
        let db = setup_test_db().await;

        // Create a zone first
        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Original Name".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Update only the name
        let updated = update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: Some("New Name".to_string()),
                players: None,
            },
        )
        .await
        .expect("Failed to update zone");

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "New Name");
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_update_audio_zone_add_players() {
        let db = setup_test_db().await;

        // Create a zone
        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Zone With Players".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Create players
        let player1_id = insert_player(&db, "Speaker 1", "output1").await;
        let player2_id = insert_player(&db, "Speaker 2", "output2").await;

        // Update zone with players
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player1_id, player2_id]),
            },
        )
        .await
        .expect("Failed to update zone");

        // Verify players are associated
        let players = get_players(&db, created.id)
            .await
            .expect("Failed to get players");
        assert_eq!(players.len(), 2);

        let player_ids: Vec<u64> = players.iter().map(|p| p.id).collect();
        assert!(player_ids.contains(&player1_id));
        assert!(player_ids.contains(&player2_id));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_update_audio_zone_remove_players() {
        let db = setup_test_db().await;

        // Create a zone
        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Zone To Modify".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Create players
        let player1_id = insert_player(&db, "Speaker 1", "output1").await;
        let player2_id = insert_player(&db, "Speaker 2", "output2").await;
        let player3_id = insert_player(&db, "Speaker 3", "output3").await;

        // Add all three players initially
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player1_id, player2_id, player3_id]),
            },
        )
        .await
        .expect("Failed to add players");

        // Verify all three are there
        let players = get_players(&db, created.id)
            .await
            .expect("Failed to get players");
        assert_eq!(players.len(), 3);

        // Now update to only keep player 1
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player1_id]),
            },
        )
        .await
        .expect("Failed to update players");

        // Verify only player 1 remains
        let players = get_players(&db, created.id)
            .await
            .expect("Failed to get players");
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].id, player1_id);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_update_audio_zone_replace_players() {
        let db = setup_test_db().await;

        // Create a zone
        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Zone To Replace".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Create players
        let player1_id = insert_player(&db, "Speaker 1", "output1").await;
        let player2_id = insert_player(&db, "Speaker 2", "output2").await;
        let player3_id = insert_player(&db, "Speaker 3", "output3").await;
        let player4_id = insert_player(&db, "Speaker 4", "output4").await;

        // Add players 1 and 2 initially
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player1_id, player2_id]),
            },
        )
        .await
        .expect("Failed to add initial players");

        // Replace with players 3 and 4
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player3_id, player4_id]),
            },
        )
        .await
        .expect("Failed to replace players");

        // Verify only players 3 and 4 are there
        let players = get_players(&db, created.id)
            .await
            .expect("Failed to get players");
        assert_eq!(players.len(), 2);

        let player_ids: Vec<u64> = players.iter().map(|p| p.id).collect();
        assert!(!player_ids.contains(&player1_id));
        assert!(!player_ids.contains(&player2_id));
        assert!(player_ids.contains(&player3_id));
        assert!(player_ids.contains(&player4_id));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_update_audio_zone_partial_player_overlap() {
        let db = setup_test_db().await;

        // Create a zone
        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Overlap Zone".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Create players
        let player1_id = insert_player(&db, "Speaker 1", "output1").await;
        let player2_id = insert_player(&db, "Speaker 2", "output2").await;
        let player3_id = insert_player(&db, "Speaker 3", "output3").await;

        // Add players 1 and 2 initially
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player1_id, player2_id]),
            },
        )
        .await
        .expect("Failed to add initial players");

        // Update to players 2 and 3 (keep 2, remove 1, add 3)
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player2_id, player3_id]),
            },
        )
        .await
        .expect("Failed to update players");

        // Verify players 2 and 3 are there
        let players = get_players(&db, created.id)
            .await
            .expect("Failed to get players");
        assert_eq!(players.len(), 2);

        let player_ids: Vec<u64> = players.iter().map(|p| p.id).collect();
        assert!(!player_ids.contains(&player1_id));
        assert!(player_ids.contains(&player2_id));
        assert!(player_ids.contains(&player3_id));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_update_audio_zone_clear_all_players() {
        let db = setup_test_db().await;

        // Create a zone
        let created = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Zone To Clear".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Create and add players
        let player1_id = insert_player(&db, "Speaker 1", "output1").await;
        let player2_id = insert_player(&db, "Speaker 2", "output2").await;

        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![player1_id, player2_id]),
            },
        )
        .await
        .expect("Failed to add players");

        // Clear all players by passing empty list
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: created.id,
                name: None,
                players: Some(vec![]),
            },
        )
        .await
        .expect("Failed to clear players");

        // Verify no players remain
        let players = get_players(&db, created.id)
            .await
            .expect("Failed to get players");
        assert!(players.is_empty());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_zone_with_sessions_joins_correctly() {
        let config_db = setup_test_db().await;
        let library_db = setup_library_db().await;

        // Create zones in config db
        let zone1 = create_audio_zone(
            &config_db,
            &CreateAudioZone {
                name: "Zone With Session".to_string(),
            },
        )
        .await
        .expect("Failed to create zone 1");
        let zone2 = create_audio_zone(
            &config_db,
            &CreateAudioZone {
                name: "Zone Without Session".to_string(),
            },
        )
        .await
        .expect("Failed to create zone 2");

        // Create sessions in library db only for zone1
        library_db
            .insert("sessions")
            .value("audio_zone_id", zone1.id)
            .execute(&*library_db)
            .await
            .expect("Failed to create session");

        // Get zones with sessions
        let zones_with_sessions = get_zone_with_sessions(&config_db, &library_db)
            .await
            .expect("Failed to get zones with sessions");

        // Only zone1 should be returned since zone2 has no session
        assert_eq!(zones_with_sessions.len(), 1);
        assert_eq!(zones_with_sessions[0].id, zone1.id);
        assert_eq!(zones_with_sessions[0].name, "Zone With Session");

        // Verify zone2 is not included
        assert!(!zones_with_sessions.iter().any(|z| z.id == zone2.id));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_zone_with_sessions_multiple_sessions() {
        let config_db = setup_test_db().await;
        let library_db = setup_library_db().await;

        // Create zones
        let zone1 = create_audio_zone(
            &config_db,
            &CreateAudioZone {
                name: "Zone 1".to_string(),
            },
        )
        .await
        .expect("Failed to create zone 1");
        let zone2 = create_audio_zone(
            &config_db,
            &CreateAudioZone {
                name: "Zone 2".to_string(),
            },
        )
        .await
        .expect("Failed to create zone 2");

        // Create sessions for both zones
        library_db
            .insert("sessions")
            .value("audio_zone_id", zone1.id)
            .execute(&*library_db)
            .await
            .expect("Failed to create session 1");
        library_db
            .insert("sessions")
            .value("audio_zone_id", zone2.id)
            .execute(&*library_db)
            .await
            .expect("Failed to create session 2");

        let zones_with_sessions = get_zone_with_sessions(&config_db, &library_db)
            .await
            .expect("Failed to get zones with sessions");

        assert_eq!(zones_with_sessions.len(), 2);

        let zone_ids: Vec<u64> = zones_with_sessions.iter().map(|z| z.id).collect();
        assert!(zone_ids.contains(&zone1.id));
        assert!(zone_ids.contains(&zone2.id));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_zone_with_sessions_no_zones() {
        let config_db = setup_test_db().await;
        let library_db = setup_library_db().await;

        let zones_with_sessions = get_zone_with_sessions(&config_db, &library_db)
            .await
            .expect("Failed to get zones with sessions");

        assert!(zones_with_sessions.is_empty());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_audio_zone_try_from_db_with_players() {
        let db = setup_test_db().await;

        // Create zone
        let zone_model = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Test Zone".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Add players to zone
        let player1_id = insert_player(&db, "Speaker 1", "output1").await;
        let player2_id = insert_player(&db, "Speaker 2", "output2").await;

        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: zone_model.id,
                name: None,
                players: Some(vec![player1_id, player2_id]),
            },
        )
        .await
        .expect("Failed to add players");

        // Convert to AudioZone domain model
        let audio_zone = audio_zone_try_from_db(zone_model, db.database.clone())
            .await
            .expect("Failed to convert to AudioZone");

        assert_eq!(audio_zone.name, "Test Zone");
        assert_eq!(audio_zone.players.len(), 2);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_audio_zone_try_from_db_without_players() {
        let db = setup_test_db().await;

        // Create zone without players
        let zone_model = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Empty Zone".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        // Convert to AudioZone domain model
        let audio_zone = audio_zone_try_from_db(zone_model, db.database.clone())
            .await
            .expect("Failed to convert to AudioZone");

        assert_eq!(audio_zone.name, "Empty Zone");
        assert!(audio_zone.players.is_empty());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_audio_zone_with_session_try_from_db() {
        let db = setup_test_db().await;

        // Create zone with players
        let zone_model = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Session Zone".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        let player_id = insert_player(&db, "Speaker", "output1").await;
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: zone_model.id,
                name: None,
                players: Some(vec![player_id]),
            },
        )
        .await
        .expect("Failed to add player");

        // Create a model with session
        let with_session_model = models::AudioZoneWithSessionModel {
            id: zone_model.id,
            session_id: 42,
            name: zone_model.name,
        };

        // Convert to domain model
        let zone_with_session =
            audio_zone_with_session_try_from_db(with_session_model, db.database.clone())
                .await
                .expect("Failed to convert to AudioZoneWithSession");

        assert_eq!(zone_with_session.name, "Session Zone");
        assert_eq!(zone_with_session.session_id, 42);
        assert_eq!(zone_with_session.players.len(), 1);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_players_for_nonexistent_zone() {
        let db = setup_test_db().await;

        // Get players for a zone that doesn't exist
        let players = get_players(&db, 999).await.expect("Failed to get players");
        assert!(players.is_empty());
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_update_zone_idempotent_players() {
        let db = setup_test_db().await;

        // Create zone
        let zone = create_audio_zone(
            &db,
            &CreateAudioZone {
                name: "Idempotent Zone".to_string(),
            },
        )
        .await
        .expect("Failed to create zone");

        let player_id = insert_player(&db, "Speaker", "output1").await;

        // Add the same player multiple times - should be idempotent
        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: zone.id,
                name: None,
                players: Some(vec![player_id]),
            },
        )
        .await
        .expect("First update failed");

        update_audio_zone(
            &db,
            UpdateAudioZone {
                id: zone.id,
                name: None,
                players: Some(vec![player_id]),
            },
        )
        .await
        .expect("Second update failed");

        // Should still only have one player
        let players = get_players(&db, zone.id)
            .await
            .expect("Failed to get players");
        assert_eq!(players.len(), 1);
    }
}
