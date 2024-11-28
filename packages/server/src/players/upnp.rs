use std::sync::LazyLock;
use std::{collections::HashMap, sync::Arc};

use moosicbox_database::profiles::PROFILES;
use moosicbox_session::update_session_audio_output_ids;
use moosicbox_upnp::UpnpDeviceScannerError;
use thiserror::Error;

use crate::{CONFIG_DB, UPNP_LISTENER_HANDLE, WS_SERVER_HANDLE};

pub static UPNP_PLAYERS: LazyLock<
    tokio::sync::RwLock<
        Vec<(
            moosicbox_audio_output::AudioOutputFactory,
            moosicbox_upnp::player::UpnpPlayer,
            moosicbox_player::PlaybackHandler,
        )>,
    >,
> = LazyLock::new(|| tokio::sync::RwLock::new(vec![]));

static SESSION_UPNP_PLAYERS: LazyLock<
    tokio::sync::RwLock<
        HashMap<
            u64,
            (
                moosicbox_audio_output::AudioOutputFactory,
                moosicbox_player::PlaybackHandler,
            ),
        >,
    >,
> = LazyLock::new(|| tokio::sync::RwLock::new(HashMap::new()));

#[derive(Debug, Error)]
pub enum InitError {
    #[error(transparent)]
    UpnpDeviceScanner(#[from] UpnpDeviceScannerError),
}

pub async fn init(
    handle: crate::ws::server::WsServerHandle,
    #[cfg(feature = "tunnel")] tunnel_handle: Option<
        moosicbox_tunnel_sender::sender::TunnelSenderHandle,
    >,
) -> Result<(), InitError> {
    load_upnp_players().await?;

    let upnp_players = {
        let binding = UPNP_PLAYERS.read().await;
        binding.iter().cloned().collect::<Vec<_>>()
    };

    log::debug!("register_upnp_player: players={}", upnp_players.len());

    for (output, _player, _) in upnp_players {
        if let Err(err) = register_upnp_player(
            handle.clone(),
            #[cfg(feature = "tunnel")]
            tunnel_handle.as_ref(),
            output,
        )
        .await
        {
            log::error!("Failed to register server player: {err:?}");
        } else {
            log::debug!("Registered server player");
        }
    }

    Ok(())
}

pub async fn load_upnp_players() -> Result<(), moosicbox_upnp::UpnpDeviceScannerError> {
    use moosicbox_audio_output::AudioOutputFactory;
    use moosicbox_player::{PlaybackHandler, PlayerSource};

    moosicbox_upnp::scan_devices().await?;

    {
        for device in moosicbox_upnp::devices().await {
            let mut players = UPNP_PLAYERS.write().await;

            if !players.iter().any(|(_, x, _)| x.device.udn() == device.udn) {
                let service_id = "urn:upnp-org:serviceId:AVTransport";
                if let Ok((device, service)) =
                    moosicbox_upnp::get_device_and_service(&device.udn, service_id)
                {
                    for profile in moosicbox_music_api::profiles::PROFILES.names() {
                        if let Some(music_apis) =
                            moosicbox_music_api::profiles::PROFILES.get(&profile)
                        {
                            let player = moosicbox_upnp::player::UpnpPlayer::new(
                                Arc::new(Box::new(music_apis)),
                                device.clone(),
                                service.clone(),
                                PlayerSource::Local,
                                UPNP_LISTENER_HANDLE.get().unwrap().clone(),
                            );

                            let playback = player.playback.clone();
                            let receiver = player.receiver.clone();

                            let output: AudioOutputFactory = player
                                .clone()
                                .try_into()
                                .expect("Failed to create audio output factory for UpnpPlayer");

                            let handler = PlaybackHandler::new(player.clone())
                                .with_playback(playback)
                                .with_output(Some(Arc::new(std::sync::Mutex::new(output.clone()))))
                                .with_receiver(receiver);

                            player
                                .playback_handler
                                .write()
                                .unwrap()
                                .replace(handler.clone());

                            players.push((output.clone(), player.clone(), handler));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_upnp_playback_update(
    update: &moosicbox_session::models::UpdateSession,
) -> std::pin::Pin<Box<dyn futures_util::Future<Output = ()> + Send>> {
    use moosicbox_player::DEFAULT_PLAYBACK_RETRY_OPTIONS;
    use moosicbox_session::get_session;

    let update = update.clone();
    let config_db = { CONFIG_DB.read().unwrap().clone().unwrap() };

    Box::pin(async move {
        log::debug!("Handling UPnP playback update={update:?}");
        let updated = {
            {
                let existing = {
                    SESSION_UPNP_PLAYERS
                        .read()
                        .await
                        .get(&update.session_id)
                        .cloned()
                };
                let audio_output_ids =
                    match update_session_audio_output_ids(&update, &config_db).await {
                        Ok(ids) => ids,
                        Err(e) => {
                            log::error!("Failed to get audio output IDs: {e:?}");
                            return;
                        }
                    };
                let existing = existing
                    .filter(|(output, _)| !audio_output_ids.iter().any(|p| p != &output.id));

                if let Some((_, player)) = existing {
                    log::debug!(
                        "handle_upnp_playback_update: Using existing player for session_id={}",
                        update.session_id
                    );
                    player
                } else {
                    log::debug!(
                        "handle_upnp_playback_update: No existing player for session_id={}",
                        update.session_id
                    );
                    if let Err(e) = load_upnp_players().await {
                        log::error!("Failed to load upnp players: {e:?}");
                        return;
                    }

                    let binding = UPNP_PLAYERS.read().await;
                    let existing = binding
                        .iter()
                        .filter(|(output, _, _)| !audio_output_ids.iter().any(|p| p != &output.id));

                    // TODO: This needs to handle multiple players
                    if let Some((output, _upnp_player, player)) = existing.into_iter().next() {
                        let mut player = player.clone();
                        let output = output.clone();
                        drop(binding);

                        if let Some(db) = PROFILES.get(&update.profile) {
                            if let Ok(Some(session)) = get_session(&db, update.session_id).await {
                                if let Err(e) = player
                                    .init_from_session(update.profile.clone(), session, &update)
                                    .await
                                {
                                    moosicbox_assert::die_or_error!(
                                        "Failed to create new player from session: {e:?}"
                                    );
                                }

                                SESSION_UPNP_PLAYERS
                                    .write()
                                    .await
                                    .insert(update.session_id, (output, player.clone()));
                            }
                        }

                        player
                    } else {
                        moosicbox_assert::die_or_panic!("No UPNP player found");
                    }
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
                update.playlist.as_ref().map(|x| {
                    x.tracks
                        .iter()
                        .map(ToOwned::to_owned)
                        .map(Into::into)
                        .collect::<Vec<_>>()
                }),
                None,
                Some(update.session_id),
                Some(update.profile),
                Some(update.playback_target),
                false,
                Some(DEFAULT_PLAYBACK_RETRY_OPTIONS),
            )
            .await
        };

        match updated {
            Ok(()) => {
                log::debug!("Updated UPnP player playback");
            }
            Err(err) => {
                log::error!("Failed to update UPnP player playback: {err:?}");
            }
        }
    })
}

#[allow(unused)]
pub async fn register_upnp_player(
    ws: crate::ws::server::WsServerHandle,
    #[cfg(feature = "tunnel")] tunnel_handle: Option<
        &moosicbox_tunnel_sender::sender::TunnelSenderHandle,
    >,
    audio_output: moosicbox_audio_output::AudioOutputFactory,
) -> Result<(), moosicbox_ws::WebsocketSendError> {
    log::debug!("register_upnp_player: Registering audio_output={audio_output:?}");
    let connection_id = "self";

    let context = moosicbox_ws::WebsocketContext {
        connection_id: connection_id.to_string(),
        ..Default::default()
    };
    let payload = vec![moosicbox_session::models::RegisterPlayer {
        name: audio_output.name,
        audio_output_id: audio_output.id,
    }];

    let handle =
        WS_SERVER_HANDLE.read().await.clone().ok_or_else(|| {
            moosicbox_ws::WebsocketSendError::Unknown("No ws server handle".into())
        })?;

    let config_db = { CONFIG_DB.read().unwrap().clone().unwrap() };
    let players = moosicbox_ws::register_players(&config_db, &handle, &context, &payload).await?;

    for player in players {
        ws.add_player_action(player.id, handle_upnp_playback_update)
            .await;

        #[cfg(feature = "tunnel")]
        if let Some(handle) = tunnel_handle {
            handle.add_player_action(player.id, handle_upnp_playback_update);
        }
    }

    for profile in PROFILES.names() {
        if let Some(db) = PROFILES.get(&profile) {
            moosicbox_ws::broadcast_sessions(&db, &handle, &context, true).await?;
        }
    }

    Ok(())
}
