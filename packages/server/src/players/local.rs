//! Local audio player initialization and management.
//!
//! This module handles initialization of local audio players for available audio output devices,
//! registers them with the server, and manages playback updates from WebSocket clients.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use moosicbox_audio_output::AudioOutputScannerError;
use moosicbox_ws::WebsocketSendError;
use switchy_database::config::ConfigDatabase;
use switchy_database::profiles::PROFILES;
use thiserror::Error;

use crate::WS_SERVER_HANDLE;

pub static SERVER_PLAYERS: LazyLock<
    tokio::sync::RwLock<
        BTreeMap<
            u64,
            (
                moosicbox_player::local::LocalPlayer,
                moosicbox_player::PlaybackHandler,
            ),
        >,
    >,
> = LazyLock::new(|| tokio::sync::RwLock::new(BTreeMap::new()));

/// Errors that can occur during local player initialization.
#[derive(Debug, Error)]
pub enum InitError {
    /// WebSocket communication error.
    #[error(transparent)]
    WebsocketSend(#[from] WebsocketSendError),
    /// Audio output device scanning error.
    #[error(transparent)]
    AudioOutputScanner(#[from] AudioOutputScannerError),
}

/// Initializes local audio players for all available audio output devices.
///
/// This function scans for available audio output devices on the system and registers a player
/// for each device. Players are stored in the [`SERVER_PLAYERS`] static map and are available
/// for playback control.
///
/// # Errors
///
/// * [`InitError::AudioOutputScanner`] - If audio device scanning fails
/// * [`InitError::WebsocketSend`] - If the WebSocket server handle is not available
pub async fn init(
    config_db: &ConfigDatabase,
    #[cfg(feature = "tunnel")] tunnel_handle: Option<
        moosicbox_tunnel_sender::sender::TunnelSenderHandle,
    >,
) -> Result<(), InitError> {
    moosicbox_audio_output::scan_outputs().await?;

    let handle =
        WS_SERVER_HANDLE.read().await.clone().ok_or_else(|| {
            moosicbox_ws::WebsocketSendError::Unknown("No ws server handle".into())
        })?;

    for audio_output in moosicbox_audio_output::output_factories().await {
        if let Err(err) = register_server_player(
            config_db,
            handle.clone(),
            #[cfg(feature = "tunnel")]
            tunnel_handle.as_ref(),
            audio_output.clone(),
        )
        .await
        {
            log::error!("Failed to register server player: {err:?}");
        } else {
            log::debug!("Registered server player audio_output={audio_output:?}");
        }
    }

    Ok(())
}

/// Handles playback update requests from WebSocket clients for local players.
///
/// This function processes session update messages (play, pause, seek, volume, etc.) and applies
/// them to the appropriate local audio player. It creates new players as needed based on the
/// audio zone configuration.
#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::too_many_lines)]
fn handle_server_playback_update(
    update: &moosicbox_session::models::UpdateSession,
) -> std::pin::Pin<Box<dyn futures_util::Future<Output = ()> + Send>> {
    use moosicbox_player::PlaybackHandler;
    use moosicbox_session::get_session;

    let update = update.clone();
    let Some(db) = PROFILES.get(&update.profile) else {
        return Box::pin(async move {});
    };

    Box::pin(async move {
        log::debug!("Handling server playback update");

        let update = update;

        let updated = {
            {
                let audio_zone =
                    match moosicbox_session::get_session_audio_zone(&db, update.session_id).await {
                        Ok(players) => players,
                        Err(e) => moosicbox_assert::die_or_panic!(
                            "Failed to get session active players: {e:?}"
                        ),
                    };

                let Some(audio_zone) = audio_zone else {
                    return;
                };

                let existing = { SERVER_PLAYERS.read().await.get(&update.session_id).cloned() };
                let existing = existing.filter(|(player, _)| {
                    player.output.as_ref().is_some_and(|output| {
                        !audio_zone
                            .players
                            .iter()
                            .any(|p| p.audio_output_id != output.lock().unwrap().id)
                    })
                });

                if let Some((_, player)) = existing {
                    player
                } else {
                    let outputs = moosicbox_audio_output::output_factories().await;

                    // TODO: handle more than one output
                    let output = audio_zone
                        .players
                        .into_iter()
                        .find_map(|x| outputs.iter().find(|output| output.id == x.audio_output_id))
                        .cloned();

                    let Some(output) = output else {
                        moosicbox_assert::die_or_panic!("No output available");
                    };

                    let mut players = SERVER_PLAYERS.write().await;

                    let local_player = match moosicbox_player::local::LocalPlayer::new(
                        moosicbox_player::PlayerSource::Local,
                        None,
                    )
                    .await
                    {
                        Ok(player) => player,
                        Err(e) => {
                            moosicbox_assert::die_or_panic!("Failed to create new player: {e:?}")
                        }
                    }
                    .with_output(output);

                    let playback = local_player.playback.clone();
                    let output = local_player.output.clone();

                    let mut player = PlaybackHandler::new(local_player.clone())
                        .with_playback(playback)
                        .with_output(output);

                    local_player
                        .playback_handler
                        .write()
                        .unwrap()
                        .replace(player.clone());

                    if let Ok(Some(session)) = get_session(&db, update.session_id).await
                        && let Err(e) = player
                            .init_from_session(update.profile.clone(), session, &update)
                            .await
                    {
                        moosicbox_assert::die_or_error!(
                            "Failed to create new player from session: {e:?}"
                        );
                    }

                    players.insert(update.session_id, (local_player, player.clone()));

                    player
                }
            }
            .update_playback(
                true,
                update.play,
                update.stop,
                update.playing,
                update.position,
                update.seek,
                update.volume,
                update
                    .playlist
                    .map(|x| x.tracks.into_iter().map(Into::into).collect()),
                None,
                Some(update.session_id),
                Some(update.profile),
                Some(update.playback_target),
                false,
                Some(moosicbox_player::DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await
        };

        match updated {
            Ok(status) => {
                log::debug!("Updated server player playback: {status:?}");
            }
            Err(err) => {
                log::error!("Failed to update server player playback: {err:?}");
            }
        }
    })
}

/// Registers a local audio player with the server.
///
/// This function creates a player connection for the specified audio output device and registers
/// it with the WebSocket server. It also sets up the playback update handler so the player can
/// receive control commands from clients.
///
/// # Errors
///
/// * [`moosicbox_ws::WebsocketSendError`] - If player registration with the WebSocket server fails
/// * [`moosicbox_ws::WebsocketSendError`] - If the WebSocket server handle is not available
pub async fn register_server_player(
    config_db: &ConfigDatabase,
    ws: crate::ws::server::WsServerHandle,
    #[cfg(feature = "tunnel")] tunnel_handle: Option<
        &moosicbox_tunnel_sender::sender::TunnelSenderHandle,
    >,
    audio_output: moosicbox_audio_output::AudioOutputFactory,
) -> Result<(), moosicbox_ws::WebsocketSendError> {
    use crate::WS_SERVER_HANDLE;

    let connection_id = "self";

    let context = moosicbox_ws::WebsocketContext {
        connection_id: connection_id.to_string(),
        ..Default::default()
    };
    let payload = moosicbox_session::models::RegisterConnection {
        connection_id: connection_id.to_string(),
        name: "MoosicBox Server".to_string(),
        players: vec![moosicbox_session::models::RegisterPlayer {
            name: audio_output.name,
            audio_output_id: audio_output.id.clone(),
        }],
    };

    let handle =
        WS_SERVER_HANDLE.read().await.clone().ok_or_else(|| {
            moosicbox_ws::WebsocketSendError::Unknown("No ws server handle".into())
        })?;

    let connection =
        moosicbox_ws::register_connection(config_db, &handle, &context, &payload).await?;

    let player = connection
        .players
        .iter()
        .find(|x| x.audio_output_id == audio_output.id)
        .ok_or_else(|| {
            moosicbox_ws::WebsocketSendError::Unknown("No player on connection".into())
        })?;

    ws.add_player_action(player.id, handle_server_playback_update)
        .await;

    #[cfg(feature = "tunnel")]
    if let Some(handle) = tunnel_handle {
        handle.add_player_action(player.id, handle_server_playback_update);
    }

    for profile in PROFILES.names() {
        if let Some(db) = PROFILES.get(&profile) {
            moosicbox_ws::broadcast_sessions(&db, &handle, &context, true).await?;
        }
    }

    Ok(())
}
