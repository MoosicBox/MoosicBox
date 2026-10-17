#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod driver;
mod fixture;
mod screenshot;

use driver::Driver;
use hyperchad_actions::logic::Value;

#[test]
fn authored_view_background_survives_projection_and_emits_opaque_paint() {
    use eframe::egui::{Color32, epaint::Shape};
    fn colors(shape: &Shape, result: &mut Vec<Color32>) {
        match shape {
            Shape::Path(path) => result.push(path.fill),
            Shape::Rect(rect) => result.push(rect.fill),
            Shape::Mesh(mesh) => result.extend(mesh.vertices.iter().map(|vertex| vertex.color)),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    colors(shape, result);
                }
            }
            _ => {}
        }
    }
    let red = hyperchad_color::Color::from_hex("#702020");
    let mut view = fixture::view();
    view.background = Some(red);
    let mut driver = Driver::new(view);
    driver.settle();
    let output = driver.step(vec![]);
    let mut fills = Vec::new();
    for shape in &output.shapes {
        colors(&shape.shape, &mut fills);
    }
    assert!(
        fills.contains(&Color32::from_rgb(112, 32, 32)),
        "authored root background absent: {fills:?}"
    );
}

#[test]
fn alternate_authored_background_changes_visible_tessellated_color() {
    use eframe::egui::{Color32, epaint::Primitive};
    fn visible_color(alternate: bool) -> Color32 {
        let mut view = fixture::view();
        if alternate {
            fixture::set_background(&mut view, hyperchad_color::Color::from_hex("#702020"));
        }
        let mut driver = Driver::new(view);
        driver.settle();
        let output = driver.step(vec![]);
        let meshes = driver
            .ctx
            .tessellate(output.shapes, output.pixels_per_point);
        let point = eframe::egui::pos2(600.0, 450.0);
        let mut color = None;
        for primitive in meshes {
            if !primitive.clip_rect.contains(point) {
                continue;
            }
            if let Primitive::Mesh(mesh) = primitive.primitive {
                for indices in mesh.indices.as_chunks::<3>().0 {
                    let vertices = [indices[0], indices[1], indices[2]]
                        .map(|index| mesh.vertices[index as usize]);
                    let triangle =
                        eframe::egui::emath::Rect::from_points(&vertices.map(|vertex| vertex.pos));
                    if triangle.contains(point)
                        && vertices.iter().all(|vertex| {
                            vertex.color == vertices[0].color && vertex.color.a() == 255
                        })
                    {
                        color = Some(vertices[0].color);
                    }
                }
            }
        }
        color.expect("visible opaque background geometry at clear sample")
    }
    assert_eq!(visible_color(false), Color32::from_rgb(24, 26, 27));
    assert_eq!(visible_color(true), Color32::from_rgb(112, 32, 32));
}

#[test]
fn production_html_labels_emit_visible_nontransparent_text_meshes() {
    use eframe::egui::epaint::Shape;
    fn collect(shape: &Shape, labels: &mut Vec<String>) {
        match shape {
            Shape::Text(text) => {
                if text.galley.job.text.trim().is_empty() {
                    return;
                }
                assert!(text.override_text_color.is_none_or(|color| color.a() > 0));
                assert!(
                    text.galley
                        .rows
                        .iter()
                        .any(|row| !row.visuals.mesh.vertices.is_empty()),
                    "text has no ink: {:?}",
                    text.galley.job.text
                );
                labels.push(text.galley.job.text.clone());
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, labels);
                }
            }
            _ => {}
        }
    }
    let mut driver = Driver::new(fixture::view());
    driver.settle();
    let output = driver.step(vec![]);
    let mut labels = Vec::new();
    for shape in &output.shapes {
        collect(&shape.shape, &mut labels);
    }
    for expected in [
        "HyperChad interaction feedback",
        "Name",
        "Enable feedback",
        "Submit feedback",
        "Next page",
    ] {
        assert!(
            labels.iter().any(|label| label.contains(expected)),
            "missing painted label {expected:?}; actual={labels:?}; diagnostics={:?}",
            driver.app.take_diagnostics()
        );
    }
    let diagnostics = driver.app.take_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|message| message.starts_with("document: DuplicateNumericId(")),
        "production paint failures: {diagnostics:?}"
    );
}

#[test]
fn production_interaction_geometry_actions_values_and_navigation() {
    let mut driver = Driver::new(fixture::view());
    let (sent, received) = std::sync::mpsc::channel();
    let router = hyperchad_router::Router::new().with_route(
        fixture::NEXT_ROUTE,
        move |request: hyperchad_router::RouteRequest| {
            let sent = sent.clone();
            async move {
                sent.send(request.path.clone()).expect("test receiver live");
                hyperchad_renderer::View::default()
            }
        },
    );
    driver.app = hyperchad_renderer_egui::feedback::FeedbackApp::new(
        fixture::view(),
        driver::FixtureCalc::default(),
    )
    .with_router(router, "/".to_owned());
    driver.settle();
    let geometry = driver.app.geometry();
    assert!(geometry[fixture::BUTTON_ID].top() > geometry[fixture::INPUT_ID].bottom());
    assert!(geometry[fixture::INPUT_ID].width() >= 200.0);
    assert!(driver.app.drain_actions().is_empty());
    assert!(driver.app.drain_navigation().is_empty());

    driver.click(fixture::BUTTON_ID);
    assert_eq!(
        driver.app.drain_actions(),
        vec![(fixture::CLICK_ACTION.to_owned(), None)]
    );
    for _ in 0..8 {
        driver.step(vec![]);
    }
    assert!(
        driver.app.drain_actions().is_empty(),
        "idle frames repeated a click"
    );
    let pos = driver.app.geometry()[fixture::BUTTON_ID].center();
    driver.step(vec![
        eframe::egui::Event::PointerMoved(pos),
        eframe::egui::Event::PointerButton {
            pos,
            button: eframe::egui::PointerButton::Primary,
            pressed: true,
            modifiers: eframe::egui::Modifiers::NONE,
        },
    ]);
    for _ in 0..8 {
        driver.step(vec![]);
    }
    assert!(
        driver.app.drain_actions().is_empty(),
        "held button emitted click before release"
    );
    driver.step(vec![eframe::egui::Event::PointerButton {
        pos,
        button: eframe::egui::PointerButton::Primary,
        pressed: false,
        modifiers: eframe::egui::Modifiers::NONE,
    }]);
    assert_eq!(
        driver.app.drain_actions(),
        vec![(fixture::CLICK_ACTION.to_owned(), None)]
    );
    driver.click(fixture::CHECKBOX_ID);
    assert_eq!(
        driver.app.drain_actions(),
        vec![(
            fixture::CHECKBOX_ACTION.to_owned(),
            Some(Value::String("true".to_owned()))
        )]
    );
    driver.click(fixture::INPUT_ID);
    driver.step(vec![eframe::egui::Event::Text(" Lovelace".to_owned())]);
    let actions = driver.app.drain_actions();
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(actions[0].0, fixture::CHANGE_ACTION);
    let Some(Value::String(value)) = &actions[0].1 else {
        panic!("missing production input value: {actions:?}");
    };
    assert_eq!(value, " Lovelace");
    for _ in 0..8 {
        driver.step(vec![]);
    }
    assert!(
        driver.app.drain_actions().is_empty(),
        "idle frames repeated change"
    );
    driver.step(vec![eframe::egui::Event::Text("!".to_owned())]);
    assert_eq!(
        driver.app.drain_actions(),
        vec![(
            fixture::CHANGE_ACTION.to_owned(),
            Some(Value::String(" Lovelace!".to_owned()))
        )]
    );
    driver.click(fixture::LINK_ID);
    assert_eq!(
        received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("real router reached without sleeps"),
        fixture::NEXT_ROUTE
    );
    assert!(
        driver.app.drain_navigation().is_empty(),
        "local navigation must not bypass R tickets"
    );
}

#[test]
fn fixture_has_unique_numeric_identity_and_zero_diagnostics() {
    fn collect(
        node: &hyperchad_transformer::Container,
        ids: &mut std::collections::BTreeSet<usize>,
    ) {
        assert!(
            ids.insert(node.id),
            "duplicate fixture numeric ID {}",
            node.id
        );
        for child in &node.children {
            collect(child, ids);
        }
    }
    collect(&fixture::view(), &mut std::collections::BTreeSet::new());
    let mut driver = Driver::new(fixture::view());
    driver.settle();
    assert!(driver.app.take_diagnostics().is_empty());
}

#[test]
fn deterministic_geometry_and_time_across_fresh_contexts() {
    let mut first = Driver::new(fixture::view());
    let mut second = Driver::new(fixture::view());
    first.settle();
    second.settle();
    assert_eq!(first.app.geometry(), second.app.geometry());
}

#[test]
fn capture_timeout_is_observable_without_sleeping() {
    use hyperchad_renderer_egui::feedback::{CaptureOptions, FeedbackApp};
    let mut driver = Driver::new(fixture::view());
    driver.app = FeedbackApp::new(fixture::view(), driver::FixtureCalc::default()).with_capture(
        CaptureOptions {
            output: std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../target/feedback-artifacts/timeout/unused.png"),
            ready: None,
            required_ids: vec!["missing-element".into()],
            max_frames: 3,
            close: false,
        },
    );
    for _ in 0..3 {
        driver.step(vec![]);
    }
    assert!(
        driver
            .app
            .capture_result()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("timed out")
    );
}
