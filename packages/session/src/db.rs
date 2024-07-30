use moosicbox_core::sqlite::{
    db::DbError,
    models::{AsModelQuery as _, Id},
};
use moosicbox_database::{
    query::{select, where_in, FilterableQuery as _, SortDirection},
    Database, DatabaseValue,
};
use moosicbox_json_utils::ToValueType as _;
use moosicbox_library::db::get_tracks;

use crate::models::{
    self, CreateSession, Player, Session, SessionPlaylist, SessionPlaylistTrack,
    SetSessionActivePlayers, UpdateSession,
};

pub async fn get_session_playlist_tracks(
    db: &dyn Database,
    session_playlist_id: u64,
) -> Result<Vec<SessionPlaylistTrack>, DbError> {
    Ok(db
        .select("session_playlist_tracks")
        .where_eq("session_playlist_id", session_playlist_id)
        .sort("id", SortDirection::Asc)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_session_playlist(
    db: &dyn Database,
    session_id: u64,
) -> Result<Option<SessionPlaylist>, DbError> {
    if let Some(ref playlist) = db
        .select("session_playlists")
        .where_eq("id", session_id)
        .execute_first(db)
        .await?
    {
        Ok(Some(playlist.as_model_query(db).await?))
    } else {
        Ok(None)
    }
}

pub async fn get_session_active_players(
    db: &dyn Database,
    session_id: u64,
) -> Result<Vec<Player>, DbError> {
    Ok(db
        .select("active_players")
        .columns(&["players.*"])
        .join("players", "players.id=active_players.player_id")
        .where_eq("active_players.session_id", session_id)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn get_session_playing(db: &dyn Database, id: u64) -> Result<Option<bool>, DbError> {
    Ok(db
        .select("sessions")
        .columns(&["playing"])
        .where_eq("id", id)
        .execute_first(db)
        .await?
        .and_then(|row| row.get("playing"))
        .map(|x| x.to_value_type() as Result<Option<bool>, _>)
        .transpose()?
        .flatten())
}

pub async fn get_session(db: &dyn Database, id: u64) -> Result<Option<Session>, DbError> {
    Ok(
        if let Some(ref session) = db
            .select("sessions")
            .where_eq("id", id)
            .execute_first(db)
            .await?
        {
            Some(session.as_model_query(db).await?)
        } else {
            None
        },
    )
}

pub async fn get_sessions(db: &dyn Database) -> Result<Vec<Session>, DbError> {
    let mut sessions = vec![];

    for ref session in db.select("sessions").execute(db).await? {
        sessions.push(session.as_model_query(db).await?);
    }

    Ok(sessions)
}

pub async fn create_session(
    db: &dyn Database,
    session: &CreateSession,
) -> Result<Session, DbError> {
    let tracks = get_tracks(
        db,
        Some(
            &session
                .playlist
                .tracks
                .iter()
                .map(|x| x.into())
                .collect::<Vec<Id>>(),
        ),
    )
    .await?;
    let playlist: SessionPlaylist = db
        .insert("session_playlists")
        .execute(db)
        .await?
        .to_value_type()?;

    for track in tracks {
        db.insert("session_playlist_tracks")
            .value("session_playlist_id", playlist.id)
            .value("track_id", track.id)
            .execute(db)
            .await?;
    }

    let new_session: Session = db
        .insert("sessions")
        .value("session_playlist_id", playlist.id)
        .value("name", session.name.clone())
        .execute(db)
        .await?
        .to_value_type()?;

    for player_id in &session.active_players {
        db.insert("active_players")
            .value("session_id", new_session.id)
            .value("player_id", *player_id)
            .execute(db)
            .await?;
    }

    Ok(Session {
        id: new_session.id,
        active: new_session.active,
        playing: new_session.playing,
        position: new_session.position,
        seek: new_session.seek,
        volume: new_session.volume,
        name: new_session.name,
        active_players: get_session_active_players(db, new_session.id).await?,
        playlist,
    })
}

pub async fn update_session(db: &dyn Database, session: &UpdateSession) -> Result<(), DbError> {
    if session.playlist.is_some() {
        log::trace!("update_session: Deleting existing session_playlist_tracks");
        db.delete("session_playlist_tracks")
            .where_in(
                "session_playlist_tracks.id",
                select("session_playlist_tracks")
                    .columns(&["session_playlist_tracks.id"])
                    .join(
                        "session_playlists",
                        "session_playlist_tracks.session_playlist_id=session_playlists.id",
                    )
                    .join(
                        "sessions",
                        "sessions.session_playlist_id=session_playlists.id",
                    )
                    .where_eq("sessions.id", session.session_id),
            )
            .execute(db)
            .await?;
    } else {
        log::trace!("update_session: No playlist");
    }

    let playlist_id = session
        .playlist
        .as_ref()
        .map(|p| p.session_playlist_id as i64);

    if let Some(tracks) = session.playlist.as_ref().map(|p| &p.tracks) {
        log::trace!("update_session: Inserting new tracks");
        for track in tracks {
            log::trace!("update_session: Inserting track {track:?}");
            db.insert("session_playlist_tracks")
                .value("session_playlist_id", playlist_id)
                .value("track_id", track.id.clone())
                .value("type", track.r#type.as_ref())
                .value("data", track.data.clone())
                .execute(db)
                .await?;
        }
    } else {
        log::trace!("update_session: No tracks to insert");
    }

    let mut values = Vec::new();

    if let Some(name) = &session.name {
        values.push(("name", DatabaseValue::String(name.clone())))
    }
    if let Some(active) = session.active {
        values.push(("active", DatabaseValue::Bool(active)))
    }
    if let Some(playing) = session.playing {
        values.push(("playing", DatabaseValue::Bool(playing)))
    }
    if let Some(position) = session.position {
        values.push(("position", DatabaseValue::Number(position as i64)))
    }
    if let Some(seek) = session.seek {
        values.push(("seek", DatabaseValue::Number(seek as i64)))
    }
    if let Some(volume) = session.volume {
        values.push(("volume", DatabaseValue::Real(volume)))
    }

    if !values.is_empty() {
        log::trace!("update_session: Updating session values values={values:?}");
        db.update("sessions")
            .where_eq("id", session.session_id)
            .values(values)
            .execute_first(db)
            .await?;
    } else {
        log::trace!("update_session: No values to update on the session");
    }

    log::trace!("update_session: Finished updating session");
    Ok(())
}

pub async fn delete_session(db: &dyn Database, session_id: u64) -> Result<(), DbError> {
    db.delete("session_playlist_tracks")
        .where_in(
            "session_playlist_tracks.id",
            select("session_playlist_tracks")
                .columns(&["session_playlist_tracks.id"])
                .join(
                    "session_playlists",
                    "session_playlist_tracks.session_playlist_id=session_playlists.id",
                )
                .join(
                    "sessions",
                    "sessions.session_playlist_id=session_playlists.id",
                )
                .where_eq("sessions.id", session_id),
        )
        .execute(db)
        .await?;

    db.delete("active_players")
        .where_eq("session_id", session_id)
        .execute(db)
        .await?
        .into_iter()
        .next()
        .ok_or(DbError::NoRow)?;

    db.delete("sessions")
        .where_eq("id", session_id)
        .execute(db)
        .await?
        .into_iter()
        .next()
        .ok_or(DbError::NoRow)?;

    db.delete("session_playlists")
        .where_eq("id", session_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_connections(db: &dyn Database) -> Result<Vec<models::Connection>, DbError> {
    let mut connections = vec![];

    for ref connection in db.select("connections").execute(db).await? {
        connections.push(connection.as_model_query(db).await?);
    }

    Ok(connections)
}

pub async fn register_connection(
    db: &dyn Database,
    connection: &models::RegisterConnection,
) -> Result<models::Connection, DbError> {
    let row: models::Connection = db
        .upsert("connections")
        .where_eq("id", connection.connection_id.clone())
        .value("id", connection.connection_id.clone())
        .value("name", connection.name.clone())
        .execute_first(db)
        .await?
        .to_value_type()?;

    for player in &connection.players {
        create_player(db, &connection.connection_id, player).await?;
    }

    Ok(models::Connection {
        id: row.id.clone(),
        name: row.name,
        created: row.created,
        updated: row.updated,
        players: get_players(db, &row.id).await?,
    })
}

pub async fn delete_connection(db: &dyn Database, connection_id: &str) -> Result<(), DbError> {
    db.delete("players")
        .where_in(
            "players.id",
            select("players")
                .columns(&["players.id"])
                .join("connections", "connections.id=players.connection_id")
                .where_eq("connections.id", connection_id),
        )
        .execute(db)
        .await?;

    db.delete("connections")
        .where_eq("id", connection_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn get_players(
    db: &dyn Database,
    connection_id: &str,
) -> Result<Vec<models::Player>, DbError> {
    Ok(db
        .select("players")
        .where_eq("connection_id", connection_id)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub async fn create_player(
    db: &dyn Database,
    connection_id: &str,
    player: &models::RegisterPlayer,
) -> Result<models::Player, DbError> {
    Ok(db
        .upsert("players")
        .where_eq("connection_id", connection_id)
        .where_eq("audio_output_id", &player.audio_output_id)
        .where_eq("name", &player.name)
        .value("connection_id", connection_id)
        .value("name", &player.name)
        .value("audio_output_id", &player.audio_output_id)
        .execute_first(db)
        .await?
        .to_value_type()?)
}

pub async fn set_session_active_players(
    db: &dyn Database,
    set_session_active_players: &SetSessionActivePlayers,
) -> Result<(), DbError> {
    db.delete("active_players")
        .where_eq("session_id", set_session_active_players.session_id)
        .execute(db)
        .await?;

    for player_id in &set_session_active_players.players {
        db.insert("active_players")
            .value("session_id", set_session_active_players.session_id)
            .value("player_id", *player_id)
            .execute(db)
            .await?;
    }

    Ok(())
}

pub async fn delete_player(db: &dyn Database, player_id: u64) -> Result<(), DbError> {
    db.delete("players")
        .where_eq("id", player_id)
        .execute(db)
        .await?;

    Ok(())
}

pub async fn delete_session_playlist_track_by_track_id(
    db: &dyn Database,
    id: u64,
) -> Result<Option<SessionPlaylistTrack>, DbError> {
    Ok(
        delete_session_playlist_tracks_by_track_id(db, Some(&vec![id]))
            .await?
            .into_iter()
            .next(),
    )
}

pub async fn delete_session_playlist_tracks_by_track_id(
    db: &dyn Database,
    ids: Option<&Vec<u64>>,
) -> Result<Vec<SessionPlaylistTrack>, DbError> {
    if ids.is_some_and(|ids| ids.is_empty()) {
        return Ok(vec![]);
    }

    Ok(db
        .delete("session_playlist_tracks")
        .where_eq("type", "'LIBRARY'")
        .filter_if_some(ids.map(|ids| where_in("track_id", ids.to_vec())))
        .execute(db)
        .await?
        .to_value_type()?)
}
