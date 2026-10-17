//! Run with `cargo run -p hyperchad_renderer_bmux --example workflows -- dashboard`.
//! The host callback authorizes only the fixture's exact in-memory artwork identity.
mod support;

use hyperchad_renderer::{Renderer, ToRenderRunner, View};
use hyperchad_renderer_bmux::{BmuxRenderer, renderer::BmuxConfig};
use support::Workflow;

fn main() -> Result<(), Box<dyn std::error::Error + Send>> {
    let workflow = match std::env::args().nth(1).as_deref() {
        None | Some("dashboard") => Workflow::Dashboard,
        Some("documents") => Workflow::Documents,
        Some("music") => Workflow::MusicLibrary,
        Some(_) => {
            return Err(Box::new(std::io::Error::other(
                "expected dashboard, documents, or music",
            )));
        }
    };
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| -> Box<dyn std::error::Error + Send> { Box::new(e) })?;
    let (action_tx, action_rx) = flume::bounded(64);
    let (resize_tx, resize_rx) = flume::bounded(64);
    let mut config = BmuxConfig::default();
    // Authorize only the fixture's exact in-memory identity, never arbitrary URLs.
    config.host.request = Some(std::sync::Arc::new(|request| {
        Box::pin(async move {
            match request {
                hyperchad_renderer_bmux::renderer::HostRequest::Resource { source, max_bytes }
                    if source == "memory:demo-cover" =>
                {
                    let mut png = std::io::Cursor::new(Vec::new());
                    image::DynamicImage::ImageRgba8(
                        image::RgbaImage::from_raw(16, 16, support::cover_rgba())
                            .expect("fixture dimensions"),
                    )
                    .write_to(&mut png, image::ImageFormat::Png)?;
                    let bytes = png.into_inner();
                    if bytes.len() > max_bytes {
                        return Err(
                            std::io::Error::other("fixture exceeds host byte budget").into()
                        );
                    }
                    Ok(hyperchad_renderer_bmux::renderer::HostResponse::Bytes(
                        bytes,
                    ))
                }
                _ => Err(std::io::Error::other("demo host denies this operation").into()),
            }
        })
    }));
    let renderer = BmuxRenderer::new(config, action_tx, resize_tx);
    let smoke = std::env::args().skip(2).any(|arg| arg == "--smoke");
    if smoke {
        let shutdown = renderer.clone();
        runtime.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _ = shutdown.shutdown().await;
        });
    }
    // A demo host drains callbacks without authorizing HTTP, files or external URLs.
    let actions = runtime.spawn(async move { while action_rx.recv_async().await.is_ok() {} });
    let resizes = runtime.spawn(async move { while resize_rx.recv_async().await.is_ok() {} });
    let producer = renderer.clone();
    let initial = runtime.spawn(async move {
        let mut root = support::document(workflow);
        if smoke {
            root.children.push(support::node(
                201,
                "smoke-secret",
                hyperchad_transformer::Element::Input {
                    input: hyperchad_transformer::Input::Password {
                        value: Some("NEVER_PAINT_PASSWORD_7e9a".into()),
                        placeholder: None,
                    },
                    name: None,
                    autofocus: None,
                },
                vec![],
            ));
            root.children.push(support::text(
                202,
                "smoke-hostile",
                "safe\u{1b}]52;c;SHOULD_NOT_EXECUTE\u{7}\u{9b}31m",
            ));
        }
        producer.render(View::from(root)).await
    });
    let navigation_renderer = renderer.clone();
    let navigation = runtime.spawn(async move {
        while let Some(path) = navigation_renderer.wait_for_navigation().await {
            let page = match path.as_str() {
                "/dashboard" => Workflow::Dashboard,
                "/documents" => Workflow::Documents,
                "/music" | "/albums/first-light" => Workflow::MusicLibrary,
                _ => continue,
            };
            if navigation_renderer
                .render(View::from(support::document(page)))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let updates_renderer = renderer.clone();
    let updates = runtime.spawn(async move {
        let mut tick = 0u64;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            tick = tick.saturating_add(1);
            if updates_renderer
                .render(
                    View::builder()
                        .with_fragment(support::text(
                            50,
                            "status",
                            &format!("Background refresh {tick}"),
                        ))
                        .build(),
                )
                .await
                .is_err()
            {
                break;
            }
            if updates_renderer
                .render_canvas(support::canvas_update())
                .await
                .is_err()
            {
                break;
            }
        }
    });
    // Handle::current is observed while deliberately entered into this runtime.
    let enter = runtime.enter();
    let handle = hyperchad_renderer::Handle::current();
    drop(enter);
    let mut runner = renderer.to_runner(handle)?;
    let result = runner.run();
    actions.abort();
    resizes.abort();
    initial.abort();
    navigation.abort();
    updates.abort();
    result
}
