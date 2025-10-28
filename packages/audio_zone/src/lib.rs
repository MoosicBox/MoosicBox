//! Audio zone management for `MoosicBox`.
//!
//! This crate provides functionality for managing audio zones, which are logical groupings
//! of audio players that can play synchronized content. Audio zones enable multi-room audio
//! playback across multiple devices.
//!
//! # Features
//!
//! * `api` - Enables Actix-web HTTP API endpoints for managing audio zones
//! * `events` - Enables event system for audio zone updates
//! * `openapi` - Enables `OpenAPI` documentation generation
//!
//! # Main Functions
//!
//! * [`zones`] - List all audio zones
//! * [`zones_with_sessions`] - List audio zones with their active playback sessions
//! * [`get_zone`] - Retrieve a specific audio zone by ID
//! * [`create_audio_zone`] - Create a new audio zone
//! * [`update_audio_zone`] - Update an existing audio zone
//! * [`delete_audio_zone`] - Delete an audio zone

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use db::{audio_zone_try_from_db, audio_zone_with_session_try_from_db};
use moosicbox_audio_zone_models::{
    AudioZone, AudioZoneWithSession, CreateAudioZone, UpdateAudioZone,
};
use moosicbox_json_utils::database::DatabaseFetchError;
use switchy_database::{config::ConfigDatabase, profiles::LibraryDatabase};

/// HTTP API endpoints for audio zone management.
///
/// Provides Actix-web route handlers for creating, reading, updating, and deleting audio zones
/// via REST API. Available when the `api` feature is enabled.
#[cfg(feature = "api")]
pub mod api;

/// Event system for audio zone updates.
///
/// Provides an event listener system that triggers callbacks when audio zones are created,
/// updated, or deleted. Available when the `events` feature is enabled.
#[cfg(feature = "events")]
pub mod events;

/// Database operations for audio zones.
///
/// Provides low-level database functions for managing audio zones, including CRUD operations
/// and conversions between database models and domain models.
pub mod db;
pub use moosicbox_audio_zone_models as models;

/// Retrieves all audio zones from the database.
///
/// # Errors
///
/// * If fails to fetch `AudioZone`s from the database
pub async fn zones(db: &ConfigDatabase) -> Result<Vec<AudioZone>, DatabaseFetchError> {
    let mut results = vec![];
    let zones = crate::db::get_zones(db).await?;
    for zone in zones {
        results.push(audio_zone_try_from_db(zone, db.into()).await?);
    }
    Ok(results)
}

/// Retrieves all audio zones along with their active playback sessions.
///
/// # Errors
///
/// * If fails to fetch `AudioZoneWithSession`s from the database
pub async fn zones_with_sessions(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
) -> Result<Vec<AudioZoneWithSession>, DatabaseFetchError> {
    let mut results = vec![];
    let zones = crate::db::get_zone_with_sessions(config_db, library_db).await?;
    for zone in zones {
        results.push(audio_zone_with_session_try_from_db(zone, config_db.into()).await?);
    }
    Ok(results)
}

/// Retrieves a specific audio zone by its ID.
///
/// Returns `None` if no audio zone exists with the given ID.
///
/// # Errors
///
/// * If fails to fetch the `AudioZone` from the database
pub async fn get_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<AudioZone>, DatabaseFetchError> {
    Ok(if let Some(zone) = crate::db::get_zone(db, id).await? {
        Some(audio_zone_try_from_db(zone, db.into()).await?)
    } else {
        None
    })
}

/// Creates a new audio zone with the specified configuration.
///
/// If the `events` feature is enabled, triggers an audio zones updated event after creation.
///
/// # Errors
///
/// * If fails to create the `AudioZone` in the database
pub async fn create_audio_zone(
    db: &ConfigDatabase,
    zone: &CreateAudioZone,
) -> Result<AudioZone, DatabaseFetchError> {
    let resp =
        audio_zone_try_from_db(crate::db::create_audio_zone(db, zone).await?, db.into()).await?;

    #[cfg(feature = "events")]
    {
        switchy_async::runtime::Handle::current().spawn_with_name(
            "create_audio_zone updated_events",
            async move {
                if let Err(e) = crate::events::trigger_audio_zones_updated_event().await {
                    moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
                }
            },
        );
    }

    Ok(resp)
}

/// Updates an existing audio zone with new configuration values.
///
/// If the `events` feature is enabled, triggers an audio zones updated event after the update.
///
/// # Errors
///
/// * If fails to update the `AudioZone` in the database
pub async fn update_audio_zone(
    db: &ConfigDatabase,
    update: UpdateAudioZone,
) -> Result<AudioZone, DatabaseFetchError> {
    let resp =
        audio_zone_try_from_db(crate::db::update_audio_zone(db, update).await?, db.into()).await?;

    #[cfg(feature = "events")]
    {
        switchy_async::runtime::Handle::current().spawn_with_name(
            "create_audio_zone updated_events",
            async move {
                if let Err(e) = crate::events::trigger_audio_zones_updated_event().await {
                    moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
                }
            },
        );
    }

    Ok(resp)
}

/// Deletes an audio zone by its ID.
///
/// Returns the deleted audio zone if it existed, or `None` if no zone with the given ID was found.
/// If the `events` feature is enabled, triggers an audio zones updated event after deletion.
///
/// # Errors
///
/// * If fails to delete the `AudioZone` from the database
pub async fn delete_audio_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<AudioZone>, DatabaseFetchError> {
    let resp = if let Some(zone) = get_zone(db, id).await? {
        crate::db::delete_audio_zone(db, id).await?;

        Some(zone)
    } else {
        None
    };

    #[cfg(feature = "events")]
    {
        switchy_async::runtime::Handle::current().spawn_with_name(
            "create_audio_zone updated_events",
            async move {
                if let Err(e) = crate::events::trigger_audio_zones_updated_event().await {
                    moosicbox_assert::die_or_error!("Failed to trigger event: {e:?}");
                }
            },
        );
    }

    Ok(resp)
}
