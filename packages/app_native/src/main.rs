use moosicbox_library_models::ApiAlbum;
use moosicbox_paging::Page;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    moosicbox_app_native_lib::app(|renderer| async move {
        let mut renderer = renderer
            .with_route("/", || async {
                moosicbox_app_native_ui::home().into_string().try_into()
            })
            .with_route("/home", || async {
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
            });

        renderer.navigate("/").await?;

        Ok::<_, Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}
