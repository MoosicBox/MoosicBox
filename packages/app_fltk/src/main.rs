use moosicbox_htmx_transformer::ElementList;

const WIDTH: u16 = 1000;
const HEIGHT: u16 = 600;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None)?;

    let mut renderer = moosicbox_renderer_fltk::Renderer::new(WIDTH, HEIGHT)?;

    let elements: ElementList = moosicbox_app_fltk_ui::home().into_string().try_into()?;

    renderer.render(elements)?;

    renderer.run()?;

    Ok(())
}
