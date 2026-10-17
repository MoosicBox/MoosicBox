//! Full production embedded surface, not a test renderer.
#[path = "../examples/support/mod.rs"]
mod support;
use bmux_keyboard::{KeyCode, KeyStroke};
use bmux_tui::{
    buffer::Buffer,
    event::Event,
    frame::Frame,
    geometry::{Rect, Size},
    image::{ImageContribution, ImagePayload},
    paint::{LocalRect, PaintCx},
};
use bmux_tui_runtime::PresentReport;
use hyperchad_renderer::{Renderer, View};
use hyperchad_renderer_bmux::{BmuxRenderer, presentation::EmbeddedSurface, renderer::BmuxConfig};
use hyperchad_transformer::{Element, Input};

fn paint(
    surface: &mut EmbeddedSurface,
    width: u16,
    height: u16,
) -> (String, Vec<ImageContribution>) {
    let ticket = surface.prepare(Size::new(width, height)).unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, width + 4, height + 4));
    let mut frame = Frame::new(&mut buffer);
    let mut cx = PaintCx::new(&mut frame);
    cx.with_child(2, 2, LocalRect::new(0, 0, width, height), |child| {
        surface.paint(ticket, child).unwrap()
    });
    let images = frame.images().to_vec();
    let text = frame
        .buffer()
        .cells()
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect();
    surface.commit(ticket, PresentReport::default()).unwrap();
    assert!(surface.commit(ticket, PresentReport::default()).is_err());
    (text, images)
}
fn key(surface: &mut EmbeddedSurface, code: KeyCode) {
    surface.event(Event::Key(KeyStroke::simple(code))).unwrap();
}

#[tokio::test]
async fn all_workflows_full_scene_keyboard_partial_edit_resize_images_canvas() {
    for workflow in [
        support::Workflow::Dashboard,
        support::Workflow::Documents,
        support::Workflow::MusicLibrary,
    ] {
        let (actions, _actions_rx) = flume::bounded(128);
        let (resizes, _resizes_rx) = flume::bounded(128);
        let renderer = BmuxRenderer::new(BmuxConfig::default(), actions, resizes);
        let mut surface = renderer.clone().into_embedded().unwrap();
        renderer
            .render(View::from(support::document(workflow)))
            .await
            .unwrap();
        surface.drain().unwrap();
        surface
            .program
            .graphics
            .register_rgba("cover", 16, 16, support::cover_rgba())
            .unwrap();
        paint(&mut surface, 120, 60);
        renderer
            .render_canvas(support::canvas_update())
            .await
            .unwrap();
        surface.drain().unwrap();
        let (text, images) = paint(&mut surface, 120, 60);
        assert!(text.contains("Ready"));
        assert!(text.contains("First Light"), "painted scene: {text:?}");
        assert!(
            text.contains("Night Walk"),
            "second library track is missing"
        );
        assert!(text.contains("Núria 星"), "Unicode artist text is missing");
        assert!(
            text.contains("· lossless"),
            "rich inline sibling is missing"
        );
        assert!(
            text.contains("locally cached"),
            "spanning summary cell is missing"
        );
        assert!(text.contains("Documents"), "navigation anchor is truncated");
        assert!(
            text.contains("Draft notes"),
            "native textarea must paint its actual value"
        );
        if cfg!(feature = "images") {
            assert!(
                images.len() >= 2,
                "cover and canvas must reach actual frame metadata"
            );
            assert!(images.iter().any(|image| matches!(image, ImageContribution::Present(p) if matches!(&p.payload, ImagePayload::Pixels { bytes, .. } if bytes.iter().any(|b| *b != 0)))));
        } else {
            assert!(
                images.is_empty(),
                "text-only build must not emit graphics metadata"
            );
            assert!(
                text.contains("Demo album"),
                "text-only scene must retain artwork alt text"
            );
            assert!(
                surface
                    .program
                    .graphics
                    .rgba("activity")
                    .unwrap()
                    .2
                    .iter()
                    .any(|byte| *byte != 0),
                "text-only builds must still apply canvas raster updates"
            );
        }
        for _ in 0..32 {
            if surface.program.controls.focused() == Some("search") {
                break;
            }
            key(&mut surface, KeyCode::Tab);
            paint(&mut surface, 120, 60);
        }
        assert_eq!(surface.program.controls.focused(), Some("search"));
        key(&mut surface, KeyCode::Char('x'));
        renderer
            .render(
                View::builder()
                    .with_fragment(support::text(50, "status", "Refreshed"))
                    .build(),
            )
            .await
            .unwrap();
        surface.drain().unwrap();
        paint(&mut surface, 60, 32);
        key(&mut surface, KeyCode::Char('y'));
        assert_eq!(
            surface.program.controls.value("search"),
            Some(hyperchad_renderer_bmux::controls::ControlValue::Text(
                "xy".into()
            ))
        );
        assert_eq!(surface.program.controls.focused(), Some("search"));
        // A failed host output must not make the ticket reusable.
        let ticket = surface.prepare(Size::new(80, 24)).unwrap();
        surface.discard();
        assert!(surface.commit(ticket, PresentReport::default()).is_err());
    }
}

#[tokio::test]
async fn decoded_image_retains_alt_text_when_host_disables_graphics() {
    let (actions, _actions_rx) = flume::bounded(64);
    let (resizes, _resizes_rx) = flume::bounded(64);
    let renderer = BmuxRenderer::new(
        BmuxConfig {
            graphics_enabled: false,
            ..BmuxConfig::default()
        },
        actions,
        resizes,
    );
    let mut surface = renderer.clone().into_embedded().unwrap();
    let mut image = support::node(
        2,
        "cover",
        Element::Image {
            source: None,
            alt: Some("Demo album artwork".into()),
            fit: None,
            source_set: None,
            sizes: None,
            loading: None,
        },
        vec![],
    );
    image.width = Some(hyperchad_transformer::Number::Integer(256));
    image.height = Some(hyperchad_transformer::Number::Integer(48));
    renderer
        .render(View::from(support::node(
            1,
            "text-host",
            Element::Div,
            vec![image],
        )))
        .await
        .unwrap();
    surface.drain().unwrap();
    surface
        .program
        .graphics
        .register_rgba("cover", 16, 16, support::cover_rgba())
        .unwrap();
    let (text, images) = paint(&mut surface, 80, 24);
    assert!(images.is_empty());
    assert!(
        text.contains("Demo album artwork"),
        "decoded images need alt fallback: {text:?}"
    );
}

#[tokio::test]
async fn native_controls_sanitize_escape_and_password_never_paints_plaintext() {
    let (actions, _actions_rx) = flume::bounded(64);
    let (resizes, _resizes_rx) = flume::bounded(64);
    let renderer = BmuxRenderer::new(BmuxConfig::default(), actions, resizes);
    let mut surface = renderer.clone().into_embedded().unwrap();
    let hostile = "ok\u{1b}]52;c;c2VjcmV0\u{7}\u{1b}[2J\u{9b}31m";
    let secret = "NEVER_PAINT_PASSWORD_7e9a";
    let root = support::node(
        1,
        "security",
        Element::Div,
        vec![
            support::node(
                2,
                "hostile-input",
                Element::Input {
                    input: Input::Text {
                        value: Some(hostile.into()),
                        placeholder: None,
                    },
                    name: None,
                    autofocus: Some(true),
                },
                vec![],
            ),
            support::node(
                3,
                "secret",
                Element::Input {
                    input: Input::Password {
                        value: Some(secret.into()),
                        placeholder: None,
                    },
                    name: None,
                    autofocus: None,
                },
                vec![],
            ),
            support::node(
                4,
                "hostile-area",
                Element::Textarea {
                    value: hostile.into(),
                    placeholder: None,
                    name: None,
                    rows: None,
                    cols: None,
                },
                vec![],
            ),
            support::node(
                5,
                "hostile-button",
                Element::Button { r#type: None },
                vec![support::text(6, "hostile-label", hostile)],
            ),
        ],
    );
    renderer.render(View::from(root)).await.unwrap();
    surface.drain().unwrap();
    let (text, _) = paint(&mut surface, 120, 24);
    assert!(!text.contains(secret));
    assert!(
        !text.chars().any(char::is_control),
        "native control leaked terminal controls: {text:?}"
    );
    surface.event(Event::Paste(hostile.into())).unwrap();
    let (text, _) = paint(&mut surface, 120, 24);
    assert!(!text.chars().any(char::is_control));
    assert!(!text.contains(secret));
}
