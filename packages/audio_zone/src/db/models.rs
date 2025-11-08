//! Database model types for audio zone operations.
//!
//! This module defines the raw database models used for querying and storing audio zone data.
//! These models are internal representations that map directly to database tables and are
//! converted to domain models for use in the public API.

use moosicbox_json_utils::{ParseError, ToValueType, database::ToValue as _};

/// Database model representing an audio zone.
///
/// This is the raw database representation used for querying and storing audio zone data.
#[derive(Debug, Clone)]
pub struct AudioZoneModel {
    /// Unique identifier for the audio zone.
    pub id: u64,
    /// Display name of the audio zone.
    pub name: String,
}

impl ToValueType<AudioZoneModel> for &switchy_database::Row {
    fn to_value_type(self) -> Result<AudioZoneModel, ParseError> {
        Ok(AudioZoneModel {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
        })
    }
}

/// Database model representing the association between an audio zone and a playback session.
///
/// This is used when querying zones with their active sessions.
#[derive(Debug, Clone)]
pub struct AudioZoneIdWithSessionIdModel {
    /// The session ID associated with this audio zone.
    pub session_id: u64,
    /// The audio zone ID.
    pub audio_zone_id: u64,
}

impl ToValueType<AudioZoneIdWithSessionIdModel> for &switchy_database::Row {
    fn to_value_type(self) -> Result<AudioZoneIdWithSessionIdModel, ParseError> {
        Ok(AudioZoneIdWithSessionIdModel {
            session_id: self.to_value("session_id")?,
            audio_zone_id: self.to_value("audio_zone_id")?,
        })
    }
}

/// Database model representing an audio zone with its associated playback session.
///
/// This combines audio zone information with session data for queries that need both.
#[derive(Debug, Clone)]
pub struct AudioZoneWithSessionModel {
    /// Unique identifier for the audio zone.
    pub id: u64,
    /// The session ID currently associated with this audio zone.
    pub session_id: u64,
    /// Display name of the audio zone.
    pub name: String,
}

impl ToValueType<AudioZoneWithSessionModel> for &switchy_database::Row {
    fn to_value_type(self) -> Result<AudioZoneWithSessionModel, ParseError> {
        Ok(AudioZoneWithSessionModel {
            id: self.to_value("id")?,
            session_id: self.to_value("session_id")?,
            name: self.to_value("name")?,
        })
    }
}

/// Database model representing the many-to-many relationship between audio zones and players.
///
/// This represents a row in the `audio_zone_players` join table.
#[derive(Debug, Clone)]
pub struct AudioZonePlayer {
    /// The audio zone ID.
    pub audio_zone_id: u64,
    /// The player ID associated with this audio zone.
    pub player_id: u64,
}

impl ToValueType<AudioZonePlayer> for &switchy_database::Row {
    fn to_value_type(self) -> Result<AudioZonePlayer, ParseError> {
        Ok(AudioZonePlayer {
            audio_zone_id: self.to_value("audio_zone_id")?,
            player_id: self.to_value("player_id")?,
        })
    }
}

/// Database model representing an audio zone and player pairing.
///
/// This is used when querying the relationship between audio zones and their associated players.
#[derive(Debug, Clone)]
pub struct AudioZoneAndPlayer {
    /// The audio zone ID.
    pub audio_zone_id: u64,
    /// The player ID.
    pub player_id: u64,
}

impl ToValueType<AudioZoneAndPlayer> for &switchy_database::Row {
    fn to_value_type(self) -> Result<AudioZoneAndPlayer, ParseError> {
        Ok(AudioZoneAndPlayer {
            audio_zone_id: self.to_value("audio_zone_id")?,
            player_id: self.to_value("player_id")?,
        })
    }
}
