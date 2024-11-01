#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{
    num::ParseIntError,
    sync::{Arc, LazyLock, OnceLock},
};

use flume::SendError;
use moosicbox_app_native_lib::{
    renderer::{Color, PartialView, Renderer, View},
    router::{ContainerElement, RouteRequest, Router},
};
use moosicbox_app_native_ui::{state, Action};
use moosicbox_env_utils::{default_env_usize, option_env_i32, option_env_u16};
use moosicbox_library_models::{ApiAlbum, ApiArtist};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_paging::Page;
use moosicbox_player::Playback;
use moosicbox_session_models::{ApiSession, UpdateSession, UpdateSessionPlaylist};
use moosicbox_ws::models::OutboundPayload;
use thiserror::Error;
use tokio::sync::RwLock;

static STATE: LazyLock<moosicbox_app_state::AppState> = LazyLock::new(|| {
    moosicbox_app_state::AppState::default()
        .with_on_current_sessions_updated_listener(current_sessions_updated)
        .with_on_after_handle_ws_message_listener(handle_ws_message)
});

static ROUTER: OnceLock<Router> = OnceLock::new();
static RENDERER: OnceLock<Arc<RwLock<Box<dyn Renderer>>>> = OnceLock::new();

async fn convert_state(app_state: &moosicbox_app_state::AppState) -> state::State {
    let mut state = state::State::default();

    if let Some(session_id) = *app_state.current_session_id.read().await {
        if let Some(session) = app_state
            .current_sessions
            .read()
            .await
            .iter()
            .find(|x| x.session_id == session_id)
        {
            state.player.playback = Some(state::PlaybackState {
                session_id,
                playing: session.playing,
                position: session.position.unwrap_or(0),
                seek: session.seek.unwrap_or(0) as f32,
                tracks: session.playlist.tracks.clone(),
            });
        }
    }

    state
}

async fn current_sessions_updated(sessions: Vec<ApiSession>) {
    log::trace!("current_sessions_updated: {sessions:?}");

    let session_id = *STATE.current_session_id.read().await;

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
    } else if let Some(first) = sessions.into_iter().next() {
        log::debug!("current_sessions_updated: setting current_session_id to first session");
        set_current_session(first).await;
    } else {
        log::debug!("current_sessions_updated: no sessions");
        STATE.current_session_id.write().await.take();
    }
}

async fn set_current_session(session: ApiSession) {
    log::debug!("set_current_session: setting current session to session={session:?}");
    STATE
        .current_session_id
        .write()
        .await
        .replace(session.session_id);

    let update = UpdateSession {
        session_id: session.session_id,
        profile: STATE.profile.read().await.clone().unwrap(),
        playback_target: moosicbox_app_state::PlaybackTarget::AudioZone { audio_zone_id: 0 },
        play: None,
        stop: None,
        name: Some(session.name.clone()),
        active: Some(session.active),
        playing: Some(session.playing),
        position: session.position,
        seek: session.seek.map(|x| x as f64),
        volume: session.volume,
        playlist: Some(UpdateSessionPlaylist {
            session_playlist_id: session.playlist.session_playlist_id,
            tracks: session
                .playlist
                .tracks
                .clone()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        }),
        quality: None,
    };

    for (id, markup) in moosicbox_app_native_ui::session_updated(&update, &session) {
        let view = PartialView {
            target: id,
            container: markup.try_into().unwrap(),
        };
        if let Err(e) = RENDERER.get().unwrap().write().await.render_partial(view) {
            log::error!("Failed to render_partial: {e:?}");
        }
    }
}

fn on_playback_event(update: &UpdateSession, _current: &Playback) {
    log::debug!("on_playback_event: received update, spawning task to handle update={update:?}");

    moosicbox_task::spawn(
        "moosicbox_app: handle_playback_event",
        handle_playback_event(update.to_owned()),
    );
}

async fn handle_ws_message(message: OutboundPayload) {
    moosicbox_logging::debug_or_trace!(
        ("handle_ws_message"),
        ("handle_ws_message: message={message:?}")
    );

    if let OutboundPayload::SessionUpdated(payload) = &message {
        handle_playback_event(payload.payload.clone().into()).await;
    }
}

async fn handle_playback_event(update: UpdateSession) {
    moosicbox_logging::debug_or_trace!(
        ("handle_playback_event"),
        ("handle_playback_event: update={update:?}")
    );

    let session = {
        STATE
            .current_sessions
            .read()
            .await
            .iter()
            .find(|x| x.session_id == update.session_id)
            .cloned()
    };

    if let Some(session) = session {
        for (id, markup) in moosicbox_app_native_ui::session_updated(&update, &session) {
            let view = PartialView {
                target: id,
                container: markup.try_into().unwrap(),
            };
            if let Err(e) = RENDERER.get().unwrap().write().await.render_partial(view) {
                log::error!("Failed to render_partial: {e:?}");
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    moosicbox_player::on_playback_event(on_playback_event);

    let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
    log::debug!("Running with {threads} max blocking threads");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(threads)
        .build()
        .unwrap();

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
        .with_route_result("/albums", |req| async move {
            Ok::<_, Box<dyn std::error::Error>>(if let Some(album_id) = req.query.get("albumId") {
                if req.query.get("full").map(|x| x.as_str()) == Some("true") {
                    let response = reqwest::get(format!(
                        "{}/menu/album?moosicboxProfile=master&albumId={album_id}",
                        std::env::var("MOOSICBOX_HOST")
                            .as_deref()
                            .unwrap_or("http://localhost:8500")
                    ))
                    .await?;

                    if !response.status().is_success() {
                        log::debug!("Error: {}", response.status());
                    }

                    let album: ApiAlbum = response.json().await?;

                    log::debug!("album: {album:?}");

                    let response = reqwest::get(format!(
                        "{}/menu/album/versions?moosicboxProfile=master&albumId={album_id}",
                        std::env::var("MOOSICBOX_HOST")
                            .as_deref()
                            .unwrap_or("http://localhost:8500")
                    ))
                    .await?;

                    if !response.status().is_success() {
                        log::debug!("Error: {}", response.status());
                    }

                    let versions: Vec<ApiAlbumVersion> = response.json().await?;

                    log::debug!("versions: {versions:?}");

                    let container: ContainerElement =
                        moosicbox_app_native_ui::albums::album_page_content(album, &versions)
                            .into_string()
                            .try_into()?;

                    container
                } else {
                    let container: ContainerElement = moosicbox_app_native_ui::albums::album(
                        &convert_state(&STATE).await,
                        album_id.parse::<u64>()?,
                    )
                    .into_string()
                    .try_into()?;

                    container
                }
            } else {
                moosicbox_app_native_ui::albums::albums(&convert_state(&STATE).await)
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
                    let response = reqwest::get(format!(
                        "{}/menu/artist?moosicboxProfile=master&artistId={artist_id}",
                        std::env::var("MOOSICBOX_HOST")
                            .as_deref()
                            .unwrap_or("http://localhost:8500")
                    ))
                    .await?;

                    if !response.status().is_success() {
                        log::debug!("Error: {}", response.status());
                    }

                    let artist: ApiArtist = response.json().await?;

                    log::debug!("artist: {artist:?}");

                    let container: ContainerElement = moosicbox_app_native_ui::artists::artist(
                        &convert_state(&STATE).await,
                        artist,
                    )
                    .into_string()
                    .try_into()?;

                    container
                } else {
                    let response = reqwest::get(format!(
                        "{}/menu/artists?moosicboxProfile=master&offset=0&limit=2000",
                        std::env::var("MOOSICBOX_HOST")
                            .as_deref()
                            .unwrap_or("http://localhost:8500")
                    ))
                    .await?;

                    if !response.status().is_success() {
                        log::debug!("Error: {}", response.status());
                    }

                    let artists: Vec<ApiArtist> = response.json().await?;

                    log::trace!("artists: {artists:?}");

                    moosicbox_app_native_ui::artists::artists(&convert_state(&STATE).await, artists)
                        .into_string()
                        .try_into()?
                },
            )
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
                client_id: None,
                signature_token: None,
                api_token: None,
                profile: Some("master".into()),
                playback_target: None,
                current_session_id: None,
            })
            .await?;

        Ok::<_, moosicbox_app_state::AppStateError>(())
    });

    let (action_tx, action_rx) = flume::unbounded();

    let mut app = moosicbox_app_native_lib::NativeAppBuilder::new()
        .with_router(router.clone())
        .with_runtime_arc(runtime.clone())
        .with_background(Color::from_hex("#181a1b"))
        .with_action_handler(move |x| {
            Ok::<_, SendError<Action>>(if let Ok(action) = Action::try_from(x) {
                action_tx.send(action)?;
                true
            } else {
                false
            })
        })
        .with_size(
            option_env_u16("WINDOW_WIDTH").unwrap().unwrap_or(1000),
            option_env_u16("WINDOW_HEIGHT").unwrap().unwrap_or(600),
        );

    let mut runner = runtime.clone().block_on(async move {
        moosicbox_task::spawn("native app action listener", async move {
            while let Ok(action) = action_rx.recv_async().await {
                match action {
                    Action::TogglePlayback => {
                        log::debug!("native app action listener: TogglePlayback");
                    }
                    Action::PreviousTrack => {
                        log::debug!("native app action listener: PreviousTrack");
                    }
                    Action::NextTrack => {
                        log::debug!("native app action listener: NextTrack");
                    }
                }
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
        let mut app = app
            .start()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;

        moosicbox_assert::assert_or_panic!(
            RENDERER.set(app.renderer.clone()).is_ok(),
            "Already set RENDERER"
        );

        log::debug!("app_native: navigating to home");
        app.router.navigate_spawn("/");

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

        app.to_runner().await
    })?;

    log::debug!("app_native: running");
    runner.run().unwrap();

    Ok(())
}

#[derive(Debug, Error)]
pub enum RouteError {
    #[error("Missing query param: '{0}'")]
    MissingQueryParam(&'static str),
    #[error("Failed to parse markup")]
    ParseMarkup,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
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
    let response = reqwest::get(format!(
        "{}/menu/albums?moosicboxProfile=master&offset={offset}&limit={limit}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8500")
    ))
    .await?;

    if !response.status().is_success() {
        log::debug!("Error: {}", response.status());
    }

    let albums: Page<ApiAlbum> = response.json().await?;

    log::trace!("albums_list_start_route: albums={albums:?}");

    moosicbox_app_native_ui::albums::albums_list_start(&albums, size)
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
    let response = reqwest::get(format!(
        "{}/menu/albums?moosicboxProfile=master&offset={offset}&limit={limit}",
        std::env::var("MOOSICBOX_HOST")
            .as_deref()
            .unwrap_or("http://localhost:8500")
    ))
    .await?;

    if !response.status().is_success() {
        log::debug!("Error: {}", response.status());
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
