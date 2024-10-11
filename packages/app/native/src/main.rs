use std::sync::Arc;

use moosicbox_env_utils::{default_env_usize, option_env_i32, option_env_u16};
use moosicbox_library_models::ApiAlbum;
use moosicbox_paging::Page;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
    log::debug!("Running with {threads} max blocking threads");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(threads)
        .build()
        .unwrap();

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

    let runtime = Arc::new(runtime);

    let mut app = moosicbox_app_native_lib::NativeApp::new()
        .with_runtime_arc(runtime.clone())
        .with_size(
            option_env_u16("WINDOW_WIDTH").unwrap().unwrap_or(1000),
            option_env_u16("WINDOW_HEIGHT").unwrap().unwrap_or(600),
        );

    if let (Some(x), Some(y)) = (
        option_env_i32("WINDOW_X").unwrap(),
        option_env_i32("WINDOW_Y").unwrap(),
    ) {
        app = app.with_position(x, y);
    }

    app = app
        .with_route(&["/", "/home"], |_| async {
            moosicbox_app_native_ui::home().into_string().try_into()
        })
        .with_route("/downloads", |_| async {
            moosicbox_app_native_ui::downloads()
                .into_string()
                .try_into()
        })
        .with_route("/albums", |_| async move {
            let response = reqwest::get(format!(
                "{}/menu/albums?moosicboxProfile=master&offset=0&limit=2000",
                std::env::var("MOOSICBOX_HOST")
                    .as_deref()
                    .unwrap_or("http://localhost:8500")
            ))
            .await?;

            if !response.status().is_success() {
                log::debug!("Error: {}", response.status());
            }

            let albums: Page<ApiAlbum> = response.json().await?;

            log::debug!("albums: {albums:?}");

            Ok::<_, Box<dyn std::error::Error>>(
                moosicbox_app_native_ui::albums(albums.items())
                    .into_string()
                    .try_into()?,
            )
        })
        .with_route("/artists", |_| async {
            moosicbox_app_native_ui::artists().into_string().try_into()
        })
        .start()?;

    app.navigate_spawn("/");

    app.run()?;

    #[cfg(feature = "bundled")]
    {
        use moosicbox_app_native_bundled::service::Commander as _;

        log::debug!("Shutting down app server..");
        if let Err(e) = app_server_handle.shutdown() {
            log::error!("AppServer failed to shutdown: {e:?}");
        }

        log::debug!("Joining app server...");
        match runtime.block_on(join_app_server) {
            Err(e) => {
                log::error!("Failed to join app server: {e:?}");
            }
            Ok(Err(e)) => {
                log::error!("Failed to join app server: {e:?}");
            }
            _ => {}
        }
    }

    Ok(())
}
