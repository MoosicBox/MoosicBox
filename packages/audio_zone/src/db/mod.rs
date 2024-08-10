use moosicbox_database::{boxed, query::*, Database, DatabaseValue};
use moosicbox_json_utils::{database::DatabaseFetchError, ToValueType};

use crate::models::{CreateAudioZone, UpdateAudioZone};

use self::models::AudioZoneModel;

pub mod models;

pub async fn update_audio_zone(
    db: &dyn Database,
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
    db: &dyn Database,
) -> Result<Vec<models::AudioZoneModel>, DatabaseFetchError> {
    Ok(db
        .select("audio_zones")
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn create_audio_zone(
    db: &dyn Database,
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
    db: &dyn Database,
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
    db: &dyn Database,
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
    db: &dyn Database,
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
