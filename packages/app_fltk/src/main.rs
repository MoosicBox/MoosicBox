const WIDTH: u16 = 1000;
const HEIGHT: u16 = 600;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    let mut renderer = moosicbox_renderer_fltk::Renderer::new(WIDTH, HEIGHT)?
        .with_route("/", || {
            moosicbox_app_fltk_ui::home()
                .into_string()
                .try_into()
                .unwrap()
        })
        .with_route("/home", || {
            moosicbox_app_fltk_ui::home()
                .into_string()
                .try_into()
                .unwrap()
        })
        .with_route("/downloads", || {
            moosicbox_app_fltk_ui::downloads()
                .into_string()
                .try_into()
                .unwrap()
        })
        .with_route("/albums", || {
            moosicbox_app_fltk_ui::albums()
                .into_string()
                .try_into()
                .unwrap()
        })
        .with_route("/artists", || {
            moosicbox_app_fltk_ui::artists()
                .into_string()
                .try_into()
                .unwrap()
        });

    renderer.navigate("/")?;

    renderer.run()?;

    Ok(())
}
