use moosicbox_database::{
    boxed, config::ConfigDatabase, profiles::LibraryDatabase, query::*, DatabaseValue,
};
use moosicbox_json_utils::{database::DatabaseFetchError, ToValueType};

use crate::models::{CreateAudioZone, UpdateAudioZone};

use self::models::AudioZoneModel;

pub mod models;

pub async fn update_audio_zone(
    db: &ConfigDatabase,
    zone: UpdateAudioZone,
) -> Result<models::AudioZoneModel, DatabaseFetchError> {
    let inserted: models::AudioZoneModel = db
        .upsert("audio_zones")
        .where_eq("id", zone.id)
        .value_opt("name", zone.name)
        .execute_first(db)
        .await?
        .to_value_type()?;

    if let Some(players) = zone.players {
        let mut existing: Vec<models::AudioZonePlayer> = db
            .select("audio_zone_players")
            .where_eq("audio_zone_id", inserted.id)
            .execute(db)
            .await?
            .to_value_type()?;

        existing.retain(|p| players.iter().any(|new_p| *new_p == p.player_id));

        db.delete("audio_zone_players")
            .where_eq("audio_zone_id", inserted.id)
            .where_not_in(
                "player_id",
                existing.iter().map(|x| x.player_id).collect::<Vec<_>>(),
            )
            .execute(db)
            .await?;

        let values = players
            .into_iter()
            .filter(|x| !existing.iter().any(|existing| existing.player_id == *x))
            .map(|x| {
                vec![
                    ("audio_zone_id", DatabaseValue::UNumber(inserted.id)),
                    ("player_id", DatabaseValue::UNumber(x)),
                ]
            })
            .collect::<Vec<_>>();

        db.upsert_multi("audio_zone_players")
            .unique(boxed![identifier("audio_zone_id"), identifier("player_id"),])
            .values(values.clone())
            .execute(db)
            .await?;
    }

    Ok(inserted)
}

pub async fn get_zones(
    db: &ConfigDatabase,
) -> Result<Vec<models::AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .select("audio_zones")
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_zone_with_sessions(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
) -> Result<Vec<models::AudioZoneWithSessionModel>, DatabaseFetchError> {
    let zones: Vec<models::AudioZoneModel> = config_db
        .select("audio_zones")
        .columns(&["audio_zones.*"])
        .execute(config_db)
        .await?
        .to_value_type()?;

    let sessions: Vec<models::AudioZoneIdWithSessionIdModel> = library_db
        .select("sessions")
        .columns(&["id as session_id", "audio_zone_id"])
        .where_in(
            "audio_zone_id",
            zones.iter().map(|x| x.id).collect::<Vec<_>>(),
        )
        .execute(library_db)
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

pub async fn create_audio_zone(
    db: &ConfigDatabase,
    zone: &CreateAudioZone,
) -> Result<AudioZoneModel, DatabaseFetchError> {
    Ok(db
        .insert("audio_zones")
        .value("name", zone.name.clone())
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn delete_audio_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .delete("audio_zones")
        .where_eq("id", id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

pub async fn get_zone(
    db: &ConfigDatabase,
    id: u64,
) -> Result<Option<models::AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .select("audio_zones")
        .where_eq("id", id)
        .execute_first(db)
        .await?
        .map(|x| x.to_value_type())
        .transpose()?)
}

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
        .execute(db)
        .await?
        .to_value_type()?)
}
