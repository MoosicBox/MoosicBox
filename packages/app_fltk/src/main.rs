use moosicbox_env_utils::{default_env_u16, default_env_usize};
use moosicbox_library_models::ApiAlbum;
use moosicbox_paging::Page;

static WIDTH: u16 = default_env_u16!("WINDOW_WIDTH", 1000);
static HEIGHT: u16 = default_env_u16!("WINDOW_HEIGHT", 600);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    let renderer = moosicbox_renderer_fltk::Renderer::new(WIDTH, HEIGHT)?
        .with_route("/", || async {
            moosicbox_app_fltk_ui::home().into_string().try_into()
        })
        .with_route("/home", || async {
            moosicbox_app_fltk_ui::home().into_string().try_into()
        })
        .with_route("/downloads", || async {
            moosicbox_app_fltk_ui::downloads().into_string().try_into()
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
                moosicbox_app_fltk_ui::albums(albums.items())
                    .into_string()
                    .try_into()?,
            )
        })
        .with_route("/artists", || async {
            moosicbox_app_fltk_ui::artists().into_string().try_into()
        });

    std::thread::spawn({
        let mut renderer = renderer.clone();
        move || {
            let threads = default_env_usize("MAX_THREADS", 64).unwrap_or(64);
            log::debug!("Running with {threads} max blocking threads");
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .max_blocking_threads(threads)
                .build()
                .unwrap();

            runtime.block_on(async move {
                renderer.navigate("/").await.map_err(|e| e.to_string())?;
                renderer.listen().await;
                Ok::<_, String>(())
            })
        }
    });

    Ok(renderer.run()?)
}
