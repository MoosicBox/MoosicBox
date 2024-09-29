use moosicbox_env_utils::default_env_u16;
use moosicbox_library_models::ApiAlbum;
use moosicbox_paging::Page;

static WIDTH: u16 = default_env_u16!("WINDOW_WIDTH", 1000);
static HEIGHT: u16 = default_env_u16!("WINDOW_HEIGHT", 600);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    let mut app = moosicbox_app_native_lib::NativeApp::new()
        .with_size(WIDTH, HEIGHT)
        .with_route(&["/", "/home"], || async {
            moosicbox_app_native_ui::home().into_string().try_into()
        })
        .with_route("/downloads", || async {
            moosicbox_app_native_ui::downloads()
                .into_string()
                .try_into()
        })
        .with_route("/albums", || async {
            let response = reqwest::get(format!(
                "{}/menu/albums?moosicboxProfile=master&offset=0&limit=10",
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
        .with_route("/artists", || async {
            moosicbox_app_native_ui::artists().into_string().try_into()
        })
        .start()?;

    app.navigate_spawn("/");

    app.run()?;

    Ok(())
}
