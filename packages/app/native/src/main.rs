#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::{num::ParseIntError, sync::Arc};

use moosicbox_app_native_lib::{
    renderer::View,
    router::{ContainerElement, RouteRequest, Router},
};
use moosicbox_env_utils::{default_env_usize, option_env_i32, option_env_u16};
use moosicbox_library_models::{ApiAlbum, ApiArtist};
use moosicbox_menu_models::api::ApiAlbumVersion;
use moosicbox_paging::Page;
use thiserror::Error;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

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
            moosicbox_app_native_ui::home()
        })
        .with_route("/downloads", |_| async {
            moosicbox_app_native_ui::downloads()
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
                        moosicbox_app_native_ui::album_page_content(album, &versions)
                            .into_string()
                            .try_into()?;

                    container
                } else {
                    let container: ContainerElement =
                        moosicbox_app_native_ui::album(album_id.parse::<u64>()?)
                            .into_string()
                            .try_into()?;

                    container
                }
            } else {
                moosicbox_app_native_ui::albums().into_string().try_into()?
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

                    let container: ContainerElement = moosicbox_app_native_ui::artist(artist)
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

                    moosicbox_app_native_ui::artists(artists)
                        .into_string()
                        .try_into()?
                },
            )
        });

    let mut app = moosicbox_app_native_lib::NativeAppBuilder::new()
        .with_router(router.clone())
        .with_runtime_arc(runtime.clone())
        .with_size(
            option_env_u16("WINDOW_WIDTH").unwrap().unwrap_or(1000),
            option_env_u16("WINDOW_HEIGHT").unwrap().unwrap_or(600),
        );

    let mut runner = runtime.clone().block_on(async move {
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

    moosicbox_app_native_ui::albums_list_start(&albums, size)
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

    moosicbox_app_native_ui::albums_list(&albums, size)
        .into_string()
        .try_into()
        .map_err(|e| {
            moosicbox_assert::die_or_error!("Failed to parse markup: {e:?}");
            RouteError::ParseMarkup
        })
}
