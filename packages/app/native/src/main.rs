// hide console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), not(feature = "windows-console")),
    windows_subsystem = "windows"
)]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    collections::HashMap,
    num::ParseIntError,
    str::FromStr,
    sync::{Arc, LazyLock, OnceLock},
};

use flume::SendError;
use hyperchad_actions::logic::Value;
use moosicbox_app_native_lib::{
    renderer::{Color, PartialView, Renderer, View},
    router::{Container, RouteRequest, Router},
};
use moosicbox_app_native_ui::{
    albums::load_albums,
    state::{self, State},
    Action, AUDIO_ZONES_CONTENT_ID, PLAYBACK_SESSIONS_CONTENT_ID,
};
use moosicbox_app_state::AppStateError;
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;
use moosicbox_env_utils::{default_env_usize, option_env_f32, option_env_i32};
use moosicbox_music_api::{profiles::PROFILES, MusicApi, SourceToMusicApi};
use moosicbox_music_models::{
    api::{ApiAlbum, ApiArtist, ApiTrack},
    AlbumSort, AlbumType, ApiSource, TrackApiSource,
};
use moosicbox_paging::Page;
use moosicbox_player::Playback;
use moosicbox_remote_library::RemoteLibraryMusicApi;
use moosicbox_session_models::{
    ApiConnection, ApiPlaybackTarget, ApiSession, ApiUpdateSession, ApiUpdateSessionPlaylist,
    UpdateSession, UpdateSessionPlaylist,
};
use moosicbox_ws::models::{InboundPayload, UpdateSessionPayload};
use thiserror::Error;

mod visualization;

static STATE: LazyLock<moosicbox_app_state::AppState> = LazyLock::new(|| {
    moosicbox_app_state::AppState::default()
        .with_on_current_sessions_updated_listener(current_sessions_updated)
        .with_on_audio_zone_with_sessions_updated_listener(audio_zone_with_sessions_updated)
        .with_on_connections_updated_listener(connections_updated)
        .with_on_after_handle_playback_update_listener(handle_playback_update)
});

static ROUTER: OnceLock<Router> = OnceLock::new();
static RENDERER: OnceLock<Box<dyn Renderer>> = OnceLock::new();

#[cfg(feature = "assets")]
mod assets {
    use std::{path::PathBuf, sync::LazyLock};

    use moosicbox_app_native_lib::renderer;

    static CARGO_MANIFEST_DIR: LazyLock<Option<std::path::PathBuf>> =
        LazyLock::new(|| std::option_env!("CARGO_MANIFEST_DIR").map(Into::into));

    static ASSETS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        CARGO_MANIFEST_DIR.as_ref().map_or_else(
            || <PathBuf as std::str::FromStr>::from_str("public").unwrap(),
            |dir| dir.join("public"),
        )
    });

    pub static ASSETS: LazyLock<Vec<renderer::assets::StaticAssetRoute>> = LazyLock::new(|| {
        vec![
            #[cfg(feature = "vanilla-js")]
            renderer::assets::StaticAssetRoute {
                route: format!(
                    "js/{}",
                    moosicbox_app_native_lib::renderer_vanilla_js::SCRIPT_NAME_HASHED.as_str()
                ),
                target: renderer::assets::AssetPathTarget::FileContents(
                    moosicbox_app_native_lib::renderer_vanilla_js::SCRIPT
                        .as_bytes()
                        .into(),
                ),
            },
            renderer::assets::StaticAssetRoute {
                route: "favicon.ico".to_string(),
                target: ASSETS_DIR.join("favicon.ico").try_into().unwrap(),
            },
            renderer::assets::StaticAssetRoute {
                route: "public".to_string(),
                target: ASSETS_DIR.clone().try_into().unwrap(),
            },
        ]
    });
}

async fn convert_state(app_state: &moosicbox_app_state::AppState) -> state::State {
    let mut state = state::State::default();

    let session = app_state.get_current_session_ref().await;
    if let Some(session) = &session {
        state.player.playback = Some(state::PlaybackState {
            session_id: session.session_id,
            playing: session.playing,
            position: session.position.unwrap_or(0),
            seek: session.seek.unwrap_or(0.0),
            volume: session.volume.unwrap_or(1.0),
            tracks: session.playlist.tracks.clone(),
        });
    }
    drop(session);

    state
}

async fn current_sessions_updated(sessions: Vec<ApiSession>) {
    log::trace!("current_sessions_updated: {sessions:?}");

    let session_id = *STATE.current_session_id.read().await;

    #[allow(clippy::collapsible_else_if)]
    if let Some(session_id) = session_id {
        if let Some(session) = sessions.into_iter().find(|x| x.session_id == session_id) {
            log::debug!("current_sessions_updated: setting current_session_id to matching session");
            set_current_session(session).await;
        } else {
            log::debug!(
                "current_sessions_updated: no matching session with session_id={session_id}"
            );
            STATE.current_session_id.write().await.take();
        }
    } else {
        if let Some(first) = sessions.into_iter().next() {
            log::debug!("current_sessions_updated: setting current_session_id to first session");
            set_current_session(first).await;
        } else {
            log::debug!("current_sessions_updated: no sessions");
            STATE.current_session_id.write().await.take();
        }
        #[cfg(any(feature = "egui", feature = "fltk"))]
        {
            log::debug!("app_native: navigating to home");
            ROUTER
                .get()
                .unwrap()
                .navigate_spawn(
                    "/",
                    moosicbox_app_native_lib::router::RequestInfo {
                        client: moosicbox_app_native_lib::CLIENT_INFO.clone(),
                    },
                )
                .await
                .expect("Failed to navigate to home")
                .expect("Failed to navigate to home");
        }
    }
}

async fn connections_updated(_connections: Vec<ApiConnection>) {
    log::trace!("connections_updated");

    refresh_audio_zone_with_sessions().await;
}

async fn audio_zone_with_sessions_updated(_zones: Vec<ApiAudioZoneWithSession>) {
    log::trace!("audio_zone_with_sessions_updated");

    refresh_audio_zone_with_sessions().await;
}

async fn refresh_audio_zone_with_sessions() {
    log::trace!("refresh_audio_zone_with_sessions");

    let zones = STATE.current_audio_zones.read().await;
    let connections = STATE.current_connections.read().await;

    update_audio_zones(&zones, &connections).await;
}

async fn set_current_session(session: ApiSession) {
    log::debug!("set_current_session: setting current session to session={session:?}");
    STATE
        .current_session_id
        .write()
        .await
        .replace(session.session_id);

    let update = ApiUpdateSession {
        session_id: session.session_id,
        profile: STATE.profile.read().await.clone().unwrap(),
        playback_target: ApiPlaybackTarget::AudioZone { audio_zone_id: 0 },
        play: None,
        stop: None,
        name: Some(session.name.clone()),
        active: Some(session.active),
        playing: Some(session.playing),
        position: session.position,
        seek: session.seek,
        volume: session.volume,
        playlist: Some(ApiUpdateSessionPlaylist {
            session_playlist_id: session.playlist.session_playlist_id,
            tracks: session.playlist.tracks.clone(),
        }),
        quality: None,
    };

    let state = convert_state(&STATE).await;

    handle_session_update(&state, &update, &session).await;

    visualization::check_visualization_update().await;
}

fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    log::debug!("on_playback_event: received update, spawning task to handle update={update:?}");

    moosicbox_task::spawn(
        "moosicbox_app: handle_playback_event",
        handle_playback_update(update.to_owned().into()),
    );
}

async fn handle_playback_update(update: ApiUpdateSession) {
    moosicbox_logging::debug_or_trace!(
        ("handle_playback_update"),
        ("handle_playback_update: update={update:?}")
    );

    moosicbox_task::spawn(
        "moosicbox_app: handle_playback_update: render partials",
        async move {
            if let Some(session) = STATE.get_current_session().await {
                let state = convert_state(&STATE).await;

                handle_session_update(&state, &update, &session).await;
            } else {
                log::debug!("handle_playback_update: no session");
            }
        },
    );

    visualization::check_visualization_update().await;
}

async fn handle_session_update(state: &State, update: &ApiUpdateSession, session: &ApiSession) {
    let renderer = RENDERER.get().unwrap();

    for (id, markup) in moosicbox_app_native_ui::session_updated(state, update, session) {
        let view = PartialView {
            target: id,
            container: markup.try_into().unwrap(),
        };
        let response = renderer.render_partial(view).await;
        if let Err(e) = response {
            log::error!("Failed to render_partial: {e:?}");
        }
    }

    if update.position.is_some() || update.playlist.is_some() {
        log::debug!("session_updated: rendering playlist session");
        update_playlist_sessions().await;

        log::debug!("handle_session_update: position or playlist updated");
        let track: Option<&ApiTrack> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize);

        if let Some(track) = track {
            if let Err(e) = renderer
                .emit_event("play-track".to_string(), Some(track.track_id.to_string()))
                .await
            {
                log::error!("Failed to emit event: {e:?}");
            }
        } else if let Err(e) = renderer.emit_event("unplay-track".to_string(), None).await {
            log::error!("Failed to emit event: {e:?}");
        }
    }
}

async fn update_audio_zones(zones: &[ApiAudioZoneWithSession], connections: &[ApiConnection]) {
    let view = PartialView {
        target: AUDIO_ZONES_CONTENT_ID.to_string(),
        container: moosicbox_app_native_ui::audio_zones::audio_zones(zones, connections)
            .try_into()
            .unwrap(),
    };
    let response = RENDERER.get().unwrap().render_partial(view).await;
    if let Err(e) = response {
        log::error!("Failed to render_partial: {e:?}");
    }
}

async fn update_playlist_sessions() {
    let view = PartialView {
        target: PLAYBACK_SESSIONS_CONTENT_ID.to_string(),
        container: moosicbox_app_native_ui::playback_sessions::playback_sessions(
            &STATE.current_sessions.read().await,
        )
        .try_into()
        .unwrap(),
    };
    let response = RENDERER.get().unwrap().render_partial(view).await;
    if let Err(e) = response {
        log::error!("Failed to render_partial: {e:?}");
    }
}

fn parse_track_sources(value: &str) -> Result<Vec<TrackApiSource>, RouteError> {
    value
        .split(',')
        .filter(|x| !x.is_empty())
        .map(TryFrom::try_from)
        .collect::<Result<Vec<_>, strum::ParseError>>()
        .map_err(|e| RouteError::RouteFailed(e.into()))
}

static PROFILE: &str = "master";

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(feature = "profiling-tracing") {
        // no global tracing defined here
    } else {
        #[allow(unused_mut)]
        let mut layers = vec![];

        #[cfg(feature = "console-subscriber")]
        if std::env::var("TOKIO_CONSOLE").as_deref() == Ok("1") {
            use moosicbox_logging::free_log_client::DynLayer;

            layers.push(Box::new(console_subscriber::spawn()) as DynLayer);
        }

        #[cfg(target_os = "android")]
        let filename = None;
        #[cfg(not(target_os = "android"))]
        let filename = Some("moosicbox_app_native.log");

        moosicbox_logging::init(filename, Some(layers)).expect("Failed to initialize FreeLog");
    }

    moosicbox_player::on_playback_event(on_playback_event);

    let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
    log::debug!("Running with {threads} max blocking threads");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(threads)
        .build()
        .unwrap();

    let mut apis_map: HashMap<ApiSource, Arc<Box<dyn MusicApi>>> = HashMap::new();

    for api_source in ApiSource::all() {
        apis_map.insert(
            *api_source,
            Arc::new(Box::new(moosicbox_music_api::CachedMusicApi::new(
                RemoteLibraryMusicApi::new(
                    std::env::var("MOOSICBOX_HOST")
                        .unwrap_or_else(|_| "http://localhost:8500".to_string()),
                    *api_source,
                    PROFILE.to_string(),
                ),
            ))),
        );
    }

    PROFILES.add(PROFILE.to_string(), Arc::new(apis_map));

    let runtime = Arc::new(runtime);

    let router = Router::new()
        .with_route(&["/", "/home"], |_| async {
            moosicbox_app_native_ui::home(&convert_state(&STATE).await)
        })
        .with_route("/downloads", |_| async {
            moosicbox_app_native_ui::downloads(&convert_state(&STATE).await)
        })
        .with_route("/settings", |_| async {
            moosicbox_app_native_ui::settings::settings(&convert_state(&STATE).await)
        })
        .with_route_result("/audio-zones", |req| async { audio_zones_route(req).await })
        .with_route_result("/playback-sessions", |req| async {
            playback_sessions_route(req).await
        })
        .with_route_result("/albums", |req| async move {
            Ok::<_, Box<dyn std::error::Error>>(if let Some(album_id) = req.query.get("albumId") {
                let source: ApiSource = req
                    .query
                    .get("source")
                    .map(TryFrom::try_from)
                    .transpose()?
                    .unwrap_or_default();

                let version_source: Option<TrackApiSource> = req
                    .query
                    .get("versionSource")
                    .map(TryFrom::try_from)
                    .transpose()?;

                let sample_rate: Option<u32> = req
                    .query
                    .get("sampleRate")
                    .map(|x| x.parse::<u32>())
                    .transpose()?;

                let bit_depth: Option<u8> = req
                    .query
                    .get("bitDepth")
                    .map(|x| x.parse::<u8>())
                    .transpose()?;

                if req.query.get("full").map(String::as_str) == Some("true") {
                    let state = convert_state(&STATE).await;
                    let album_id = album_id.into();
                    let api = PROFILES.get(PROFILE).unwrap().get(source)?;
                    let album = api
                        .album(&album_id)
                        .await?
                        .ok_or_else(|| {
                            RouteError::RouteFailed(
                                format!("No album for album_id={album_id}").into(),
                            )
                        })?
                        .into();

                    log::debug!("album: {album:?}");

                    let versions = api
                        .album_versions(&album_id, None, None)
                        .await?
                        .map(Into::into);

                    log::debug!("versions: {versions:?}");

                    let container: Container = moosicbox_app_native_ui::albums::album_page_content(
                        &state,
                        &album,
                        &versions,
                        versions.iter().find(|v| {
                            version_source.is_none_or(|x| v.source == x)
                                && bit_depth.is_none_or(|x| v.bit_depth.is_some_and(|b| b == x))
                                && sample_rate.is_none_or(|x| v.sample_rate.is_some_and(|s| s == x))
                        }),
                    )
                    .into_string()
                    .try_into()?;

                    container
                } else {
                    let container: Container = moosicbox_app_native_ui::albums::album(
                        &convert_state(&STATE).await,
                        album_id,
                        Some(source),
                        version_source,
                        sample_rate,
                        bit_depth,
                    )
                    .into_string()
                    .try_into()?;

                    container
                }
            } else {
                let filtered_sources = parse_track_sources(
                    req.query
                        .get("sources")
                        .map(String::as_str)
                        .unwrap_or_default(),
                )?;
                let sort = req
                    .query
                    .get("sort")
                    .map(String::as_str)
                    .map(FromStr::from_str)
                    .and_then(Result::ok)
                    .unwrap_or(AlbumSort::NameAsc);

                moosicbox_app_native_ui::albums::albums(
                    &convert_state(&STATE).await,
                    &filtered_sources,
                    sort,
                )
                .into_string()
                .try_into()?
            })
        })
        .with_route_result("/albums-list-start", |req| async move {
            albums_list_start_route(req).await
        })
        .with_route_result(
            "/albums-list",
            |req| async move { albums_list_route(req).await },
        )
        .with_route_result("/artists", |req| async move {
            Ok::<_, Box<dyn std::error::Error>>(
                if let Some(artist_id) = req.query.get("artistId") {
                    let source: Option<ApiSource> =
                        req.query.get("source").map(TryFrom::try_from).transpose()?;

                    let response = reqwest::get(format!(
                        "{}/menu/artist?moosicboxProfile={PROFILE}&artistId={artist_id}{}",
                        std::env::var("MOOSICBOX_HOST")
                            .as_deref()
                            .unwrap_or("http://localhost:8500"),
                        source.map_or_else(String::new, |x| format!("&source={x}")),
                    ))
                    .await?;

                    if !response.status().is_success() {
                        let message =
                            format!("Error: {} {}", response.status(), response.text().await?);
                        log::error!("{message}");
                        return Err(RouteError::RouteFailed(message.into()).into());
                    }

                    let artist: ApiArtist = response.json().await?;

                    log::debug!("artist: {artist:?}");

                    let container: Container = moosicbox_app_native_ui::artists::artist(
                        &convert_state(&STATE).await,
                        &artist,
                    )
                    .into_string()
                    .try_into()?;

                    container
                } else {
                    let response = reqwest::get(format!(
                        "{}/menu/artists?moosicboxProfile={PROFILE}&offset=0&limit=2000",
                        std::env::var("MOOSICBOX_HOST")
                            .as_deref()
                            .unwrap_or("http://localhost:8500")
                    ))
                    .await?;

                    if !response.status().is_success() {
                        let message =
                            format!("Error: {} {}", response.status(), response.text().await?);
                        log::error!("{message}");
                        return Err(RouteError::RouteFailed(message.into()).into());
                    }

                    let artists: Vec<ApiArtist> = response.json().await?;

                    log::trace!("artists: {artists:?}");

                    moosicbox_app_native_ui::artists::artists(
                        &convert_state(&STATE).await,
                        &artists,
                    )
                    .into_string()
                    .try_into()?
                },
            )
        })
        .with_route_result("/artists/albums-list", |req| async move {
            artist_albums_list_route(req).await
        });

    moosicbox_assert::assert_or_panic!(ROUTER.set(router.clone()).is_ok(), "Already set ROUTER");

    moosicbox_task::spawn_on("Initialize AppState", runtime.handle(), async move {
        STATE
            .set_state(moosicbox_app_state::UpdateAppState {
                connection_id: Some("123".into()),
                connection_name: Some("Test Egui".into()),
                api_url: Some(
                    std::env::var("MOOSICBOX_HOST")
                        .as_deref()
                        .unwrap_or("http://localhost:8500")
                        .to_string(),
                ),
                client_id: std::env::var("MOOSICBOX_CLIENT_ID").ok(),
                signature_token: std::env::var("MOOSICBOX_SIGNATURE_TOKEN").ok(),
                api_token: std::env::var("MOOSICBOX_API_TOKEN").ok(),
                profile: Some(PROFILE.to_string()),
                playback_target: None,
                current_session_id: None,
            })
            .await?;

        Ok::<_, moosicbox_app_state::AppStateError>(())
    });

    let (action_tx, action_rx) = flume::unbounded();

    let width = option_env_f32("WINDOW_WIDTH").unwrap().unwrap_or(1000.0);
    let height = option_env_f32("WINDOW_HEIGHT").unwrap().unwrap_or(600.0);

    let mut app = moosicbox_app_native_lib::NativeAppBuilder::new()
        .with_router(router)
        .with_runtime_arc(runtime.clone())
        .with_background(Color::from_hex("#181a1b"))
        .with_action_handler(move |x, value| {
            Ok::<_, SendError<(Action, Option<Value>)>>(if let Ok(action) = Action::try_from(x) {
                action_tx.send((action, value.cloned()))?;
                true
            } else {
                false
            })
        })
        .with_size(width, height);

    visualization::set_dimensions(width, height);

    #[cfg(feature = "assets")]
    {
        for assets in assets::ASSETS.iter().cloned() {
            app = app.with_static_asset_route_result(assets).unwrap();
        }
    }

    #[cfg(not(feature = "bundled"))]
    let runner_runtime = runtime;
    #[cfg(feature = "bundled")]
    let runner_runtime = runtime.clone();

    let mut runner = runner_runtime.block_on(async move {
        moosicbox_task::spawn("native app action listener", async move {
            while let Ok((action, value)) = action_rx.recv_async().await {
                if let Err(e) = handle_action(action, value).await {
                    log::error!("Failed to handle action: {e:?}");
                };
            }
        });

        #[cfg(feature = "bundled")]
        let (join_app_server, app_server_handle) = {
            use moosicbox_app_native_bundled::service::Commander as _;

            log::debug!("Starting app server");

            let context = moosicbox_app_native_bundled::Context::new(runtime.handle());
            let server = moosicbox_app_native_bundled::service::Service::new(context);

            let app_server_handle = server.handle();
            let (tx, rx) = tokio::sync::oneshot::channel();

            let join_app_server = server.start_on(runtime.handle());

            app_server_handle
                .send_command(moosicbox_app_native_bundled::Command::WaitForStartup { sender: tx })
                .expect("Failed to send WaitForStartup command");

            log::debug!("Waiting for app server to start");

            runtime.block_on(rx).expect("Failed to start app server");

            log::debug!("App server started");

            (join_app_server, app_server_handle)
        };

        if let (Some(x), Some(y)) = (
            option_env_i32("WINDOW_X").unwrap(),
            option_env_i32("WINDOW_Y").unwrap(),
        ) {
            app = app.with_position(x, y);
        }
        log::debug!("app_native: setting up routes");

        log::debug!("app_native: starting app");
        let app = app
            .start()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

        moosicbox_assert::assert_or_panic!(
            RENDERER.set(app.renderer.clone().into()).is_ok(),
            "Already set RENDERER"
        );

        #[cfg(feature = "bundled")]
        {
            use moosicbox_app_native_bundled::service::Commander as _;

            log::debug!("Shutting down app server..");
            if let Err(e) = app_server_handle.shutdown() {
                moosicbox_assert::die_or_error!("AppServer failed to shutdown: {e:?}");
            }

            log::debug!("Joining app server...");
            match runtime.block_on(join_app_server) {
                Err(e) => {
                    moosicbox_assert::die_or_error!("Failed to join app server: {e:?}");
                }
                Ok(Err(e)) => {
                    moosicbox_assert::die_or_error!("Failed to join app server: {e:?}");
                }
                _ => {}
            }
        }

        app.to_runner()
    })?;

    log::debug!("app_native: running");
    runner.run().unwrap();

    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn handle_action(action: Action, value: Option<Value>) -> Result<(), AppStateError> {
    log::debug!("handle_action: {action:?}");

    match &action {
        Action::RefreshVisualization => {
            #[cfg(feature = "_calculated_canvas")]
            {
                let renderer = RENDERER.get().unwrap();
                let (width, height) = if let Some(visualization) =
                    renderer.container().find_element_by_str_id("visualization")
                {
                    (
                        visualization.calculated_width.unwrap(),
                        visualization.calculated_height.unwrap(),
                    )
                } else {
                    return Ok(());
                };

                log::debug!("handle_action: updating visualization width={width} height={height}");

                visualization::set_dimensions(width, height);
                visualization::check_visualization_update().await;
            }
            Ok(())
        }
        Action::TogglePlayback
        | Action::PreviousTrack
        | Action::NextTrack
        | Action::SetVolume
        | Action::SeekCurrentTrackPercent
        | Action::PlayAlbum { .. }
        | Action::AddAlbumToQueue { .. }
        | Action::PlayAlbumStartingAtTrackId { .. }
        | Action::PlayTracks { .. } => {
            let Some(session) = STATE.get_current_session().await else {
                log::debug!("handle_action: no current session");
                return Ok(());
            };
            let Some(profile) = STATE.profile.read().await.clone() else {
                log::debug!("handle_action: no current session");
                return Ok(());
            };

            let playback_target =
                { STATE.current_playback_target.read().await.clone() }.or(session.playback_target);

            let Some(playback_target) = playback_target else {
                log::debug!("handle_action: no playback_target");
                return Ok(());
            };

            match &action {
                Action::RefreshVisualization | Action::FilterAlbums { .. } => unreachable!(),
                Action::TogglePlayback => {
                    STATE
                        .queue_ws_message(
                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                payload: UpdateSession {
                                    session_id: session.session_id,
                                    profile,
                                    playback_target,
                                    play: None,
                                    stop: None,
                                    name: None,
                                    active: None,
                                    playing: Some(!session.playing),
                                    position: None,
                                    seek: None,
                                    volume: None,
                                    playlist: None,
                                    quality: None,
                                },
                            }),
                            true,
                        )
                        .await
                }
                Action::PreviousTrack => {
                    if let Some(position) = session.position {
                        let seek = session.seek.unwrap_or(0.0);
                        let position = if seek < 5.0 && position > 0 {
                            position - 1
                        } else {
                            position
                        };

                        STATE
                            .queue_ws_message(
                                InboundPayload::UpdateSession(UpdateSessionPayload {
                                    payload: UpdateSession {
                                        session_id: session.session_id,
                                        profile,
                                        playback_target,
                                        play: None,
                                        stop: None,
                                        name: None,
                                        active: None,
                                        playing: None,
                                        position: Some(position),
                                        seek: Some(0.0),
                                        volume: None,
                                        playlist: None,
                                        quality: None,
                                    },
                                }),
                                true,
                            )
                            .await
                    } else {
                        Ok(())
                    }
                }
                Action::NextTrack => {
                    if let Some(position) = session.position {
                        if usize::from(position) + 1 >= session.playlist.tracks.len() {
                            log::debug!("handle_action: already at last track");
                            return Ok(());
                        }
                        STATE
                            .queue_ws_message(
                                InboundPayload::UpdateSession(UpdateSessionPayload {
                                    payload: UpdateSession {
                                        session_id: session.session_id,
                                        profile,
                                        playback_target,
                                        play: None,
                                        stop: None,
                                        name: None,
                                        active: None,
                                        playing: None,
                                        position: Some(position + 1),
                                        seek: Some(0.0),
                                        volume: None,
                                        playlist: None,
                                        quality: None,
                                    },
                                }),
                                true,
                            )
                            .await
                    } else {
                        Ok(())
                    }
                }
                Action::SetVolume => {
                    log::debug!("handle_action: SetVolume: {value:?}");
                    let volume = value
                        .expect("Missing volume value")
                        .as_f32(
                            None::<
                                &Box<dyn Fn(&hyperchad_actions::logic::CalcValue) -> Option<Value>>,
                            >,
                        )
                        .expect("Invalid volume value");
                    if STATE.get_current_session().await.is_some_and(|x| {
                        x.volume
                            .is_some_and(|x| (x - f64::from(volume)).abs() < 0.01)
                    }) {
                        log::debug!("handle_action: SetVolume: already at desired volume");
                        Ok(())
                    } else {
                        STATE
                            .queue_ws_message(
                                InboundPayload::UpdateSession(UpdateSessionPayload {
                                    payload: UpdateSession {
                                        session_id: session.session_id,
                                        profile,
                                        playback_target,
                                        play: None,
                                        stop: None,
                                        name: None,
                                        active: None,
                                        playing: None,
                                        position: None,
                                        seek: None,
                                        volume: Some(f64::from(volume)),
                                        playlist: None,
                                        quality: None,
                                    },
                                }),
                                true,
                            )
                            .await
                    }
                }
                Action::SeekCurrentTrackPercent => {
                    log::debug!("handle_action: SeekCurrentTrackPercent: {value:?}");
                    let seek = value
                        .expect("Missing seek value")
                        .as_f32(
                            None::<
                                &Box<dyn Fn(&hyperchad_actions::logic::CalcValue) -> Option<Value>>,
                            >,
                        )
                        .expect("Invalid seek value");
                    let session = STATE.get_current_session_ref().await;
                    if let Some(session) = session {
                        if let Some(position) = session.position {
                            if let Some(duration) = session
                                .playlist
                                .tracks
                                .get(position as usize)
                                .map(|x| x.duration)
                            {
                                let seek = duration * f64::from(seek);

                                if seek < 0.0 || seek > duration {
                                    log::debug!("handle_action: SeekCurrentTrackPercent: target seek is out of track duration bounds");
                                    Ok(())
                                } else if session.seek.is_some_and(|x| (x - seek).abs() < 0.1) {
                                    log::debug!("handle_action: SeekCurrentTrackPercent: already at desired position");
                                    Ok(())
                                } else {
                                    STATE
                                        .queue_ws_message(
                                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                                payload: UpdateSession {
                                                    session_id: session.session_id,
                                                    profile,
                                                    playback_target,
                                                    play: None,
                                                    stop: None,
                                                    name: None,
                                                    active: None,
                                                    playing: None,
                                                    position: None,
                                                    seek: Some(seek),
                                                    volume: None,
                                                    playlist: None,
                                                    quality: None,
                                                },
                                            }),
                                            true,
                                        )
                                        .await
                                }
                            } else {
                                log::debug!("handle_action: SeekCurrentTrackPercent: no track");
                                Ok(())
                            }
                        } else {
                            log::debug!("handle_action: SeekCurrentTrackPercent: no position");
                            Ok(())
                        }
                    } else {
                        log::debug!("handle_action: SeekCurrentTrackPercent: no session");
                        Ok(())
                    }
                }
                Action::PlayAlbum {
                    album_id,
                    api_source,
                    version_source,
                    sample_rate,
                    bit_depth,
                }
                | Action::PlayAlbumStartingAtTrackId {
                    album_id,
                    api_source,
                    version_source,
                    sample_rate,
                    bit_depth,
                    ..
                }
                | Action::AddAlbumToQueue {
                    album_id,
                    api_source,
                    version_source,
                    sample_rate,
                    bit_depth,
                } => {
                    let api = PROFILES
                        .get(PROFILE)
                        .unwrap()
                        .get(*api_source)
                        .map_err(|e| AppStateError::unknown(e.to_string()))?;
                    let versions = api
                        .album_versions(album_id, None, None)
                        .await
                        .map_err(|e| AppStateError::unknown(e.to_string()))?
                        .clone();
                    let Some(version) = versions
                        .iter()
                        .find(|x| {
                            version_source.is_none_or(|y| x.source == y)
                                && sample_rate.is_none_or(|y| x.sample_rate.is_some_and(|x| x == y))
                                && bit_depth.is_none_or(|y| x.bit_depth.is_some_and(|x| x == y))
                        })
                        .or_else(|| versions.first())
                        .cloned()
                    else {
                        log::debug!("handle_action: no album tracks");
                        return Ok(());
                    };

                    let play = matches!(
                        action,
                        Action::PlayAlbum { .. } | Action::PlayAlbumStartingAtTrackId { .. }
                    );

                    let tracks =
                        if let Action::PlayAlbumStartingAtTrackId { start_track_id, .. } = action {
                            if let Some(index) =
                                version.tracks.iter().position(|x| x.id == start_track_id)
                            {
                                version.tracks.into_iter().skip(index).collect()
                            } else {
                                vec![]
                            }
                        } else {
                            version.tracks
                        };

                    let mut tracks = tracks.into_iter().map(Into::into).collect();

                    if !play {
                        tracks = [session.playlist.tracks, tracks].concat();
                    }

                    let position = if play { Some(0) } else { None };
                    let seek = if play { Some(0.0) } else { None };

                    STATE
                        .queue_ws_message(
                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                payload: UpdateSession {
                                    session_id: session.session_id,
                                    profile,
                                    playback_target,
                                    play: Some(play),
                                    stop: None,
                                    name: None,
                                    active: None,
                                    playing: None,
                                    position,
                                    seek,
                                    volume: None,
                                    playlist: Some(UpdateSessionPlaylist {
                                        session_playlist_id: session.playlist.session_playlist_id,
                                        tracks,
                                    }),
                                    quality: None,
                                },
                            }),
                            true,
                        )
                        .await
                }
                Action::PlayTracks {
                    track_ids,
                    api_source,
                } => {
                    let api = PROFILES
                        .get(PROFILE)
                        .unwrap()
                        .get(*api_source)
                        .map_err(|e| AppStateError::unknown(e.to_string()))?;
                    let tracks = api
                        .tracks(Some(track_ids), None, None, None, None)
                        .await
                        .map_err(|e| AppStateError::unknown(e.to_string()))?
                        .map(Into::into)
                        .items()
                        .to_vec();

                    let position = Some(0);
                    let seek = Some(0.0);

                    STATE
                        .queue_ws_message(
                            InboundPayload::UpdateSession(UpdateSessionPayload {
                                payload: UpdateSession {
                                    session_id: session.session_id,
                                    profile,
                                    playback_target,
                                    play: Some(true),
                                    stop: None,
                                    name: None,
                                    active: None,
                                    playing: None,
                                    position,
                                    seek,
                                    volume: None,
                                    playlist: Some(UpdateSessionPlaylist {
                                        session_playlist_id: session.playlist.session_playlist_id,
                                        tracks,
                                    }),
                                    quality: None,
                                },
                            }),
                            true,
                        )
                        .await
                }
            }
        }
        Action::FilterAlbums {
            filtered_sources,
            sort,
        } => {
            let value = value.expect("Missing filter value");
            let filter = value.as_str().expect("Invalid filter value");
            log::debug!("handle_action: FilterAlbums filter={filter}");

            let size: u16 = 200;

            let view = PartialView {
                target: "albums".to_string(),
                container: load_albums(size, *sort, filtered_sources, filter)
                    .try_into()
                    .unwrap(),
            };
            let response = RENDERER.get().unwrap().render_partial(view).await;
            if let Err(e) = response {
                log::error!("Failed to render_partial: {e:?}");
            }

            Ok(())
        }
    }
}

#[derive(Debug, Error)]
pub enum RouteError {
    #[error("Missing query param: '{0}'")]
    MissingQueryParam(&'static str),
    #[error("Failed to parse markup")]
    ParseMarkup,
    #[error(transparent)]
    StrumParse(#[from] strum::ParseError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Route failed: {0:?}")]
    RouteFailed(Box<dyn std::error::Error>),
}

async fn albums_list_start_route(req: RouteRequest) -> Result<View, RouteError> {
    let Some(limit) = req.query.get("limit") else {
        return Err(RouteError::MissingQueryParam("limit"));
    };
    let limit = limit.parse::<u32>()?;
    let Some(size) = req.query.get("size") else {
        return Err(RouteError::MissingQueryParam("size"));
    };
    let size = size.parse::<u16>()?;
    let offset = if let Some(offset) = req.query.get("offset") {
        offset.parse::<u32>()?
    } else {
        0
    };
    let search = req.query.get("search").filter(|x| !x.is_empty());

    let filtered_sources = parse_track_sources(
        req.query
            .get("sources")
            .map(String::as_str)
            .unwrap_or_default(),
    )?;

    let sort = req
        .query
        .get("sort")
        .map(String::as_str)
        .map(FromStr::from_str)
        .and_then(Result::ok)
        .unwrap_or(AlbumSort::NameAsc);

    let response = reqwest::get(format!(
        "{}/menu/albums?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}{}&sort={sort}{}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8500"),
        if filtered_sources.is_empty() {
            String::new()
        } else {
            format!(
                "&sources={}",
                filtered_sources
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        },
        search.map_or_else(String::new, |search| format!("&search={search}"))
    ))
    .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_start_route: albums={albums:?}");

    moosicbox_app_native_ui::albums::albums_list_start(
        &albums,
        &filtered_sources,
        sort,
        size,
        search.map_or("", |search| search),
    )
    .into_string()
    .try_into()
    .map_err(|e| {
        moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
        RouteError::ParseMarkup
    })
}

async fn albums_list_route(req: RouteRequest) -> Result<View, RouteError> {
    let Some(offset) = req.query.get("offset") else {
        return Err(RouteError::MissingQueryParam("offset"));
    };
    let offset = offset.parse::<u32>()?;
    let Some(limit) = req.query.get("limit") else {
        return Err(RouteError::MissingQueryParam("limit"));
    };
    let limit = limit.parse::<u32>()?;
    let Some(size) = req.query.get("size") else {
        return Err(RouteError::MissingQueryParam("size"));
    };
    let size = size.parse::<u16>()?;

    let search = req.query.get("search").filter(|x| !x.is_empty());

    let filtered_sources = parse_track_sources(
        req.query
            .get("sources")
            .map(String::as_str)
            .unwrap_or_default(),
    )?;

    let sort = req
        .query
        .get("sort")
        .map(String::as_str)
        .map(FromStr::from_str)
        .and_then(Result::ok)
        .unwrap_or(AlbumSort::NameAsc);

    let response = reqwest::get(format!(
        "{}/menu/albums?moosicboxProfile={PROFILE}&offset={offset}&limit={limit}{}&sort={sort}{}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8500"),
        if filtered_sources.is_empty() {
            String::new()
        } else {
            format!(
                "&sources={}",
                filtered_sources
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        },
        search.map_or_else(String::new, |search| format!("&search={search}"))
    ))
    .await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_route: albums={albums:?}");

    moosicbox_app_native_ui::albums::albums_list(&albums, size)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

async fn artist_albums_list_route(req: RouteRequest) -> Result<View, RouteError> {
    let Some(artist_id) = req.query.get("artistId") else {
        return Err(RouteError::MissingQueryParam("artistId"));
    };
    let source: ApiSource = req
        .query
        .get("source")
        .map(TryFrom::try_from)
        .transpose()?
        .ok_or(RouteError::MissingQueryParam("Missing source query param"))?;
    let album_type: AlbumType = req
        .query
        .get("albumType")
        .map(String::as_str)
        .map(TryFrom::try_from)
        .transpose()?
        .ok_or(RouteError::MissingQueryParam(
            "Missing albumType query param",
        ))?;
    let Some(size) = req.query.get("size") else {
        return Err(RouteError::MissingQueryParam("size"));
    };
    let size = size.parse::<u16>()?;
    let url = format!(
        "{}/menu/albums?moosicboxProfile={PROFILE}&artistId={artist_id}&source={source}&albumType={album_type}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8500")
    );
    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_route: albums={albums:?}");

    moosicbox_app_native_ui::artists::albums_list(&albums, source, album_type, size)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

async fn audio_zones_route(_req: RouteRequest) -> Result<View, RouteError> {
    let url = format!(
        "{}/audio-zone/with-session?moosicboxProfile={PROFILE}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8500")
    );
    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let zones: Page<ApiAudioZoneWithSession> = response.json().await?;

    moosicbox_app_native_ui::audio_zones::audio_zones(&zones, &[])
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}

async fn playback_sessions_route(_req: RouteRequest) -> Result<View, RouteError> {
    let url = format!(
        "{}/session/sessions?moosicboxProfile={PROFILE}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8500")
    );
    let response = reqwest::get(url).await?;

    if !response.status().is_success() {
        let message = format!("Error: {} {}", response.status(), response.text().await?);
        log::error!("{message}");
        return Err(RouteError::RouteFailed(message.into()));
    }

    let sessions: Page<ApiSession> = response.json().await?;

    moosicbox_app_native_ui::playback_sessions::playback_sessions(&sessions)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}
